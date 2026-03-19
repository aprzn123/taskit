use std::{mem, sync::mpsc, thread, time::Duration};

use crossterm::event::{self, Event as CEvent};
use ratatui::Frame;
use smallvec::SmallVec;

use crate::common::error::{Source, TaskitResult, With};

pub mod sync {
    use std::{sync::mpsc::{self, RecvTimeoutError}, time::Duration};

    pub struct ExternalFunction<T, U> {
        send: mpsc::Sender<T>,
        recv: mpsc::Receiver<U>,
    }

    pub struct ExternalFunctionListener<T, U, F> {
        recv: mpsc::Receiver<T>,
        send: mpsc::Sender<U>,
        behavior: F,
    }
    
    pub fn function<T: Send, U: Send, F: Fn(T) -> U>(f: F) -> (ExternalFunction<T, U>, ExternalFunctionListener<T, U, F>) {
        let (call_tx, call_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        (ExternalFunction { send: call_tx, recv: res_rx }, ExternalFunctionListener { recv: call_rx, send: res_tx, behavior: f })
    }
    
    impl<T: Send, U: Send> ExternalFunction<T, U> {
        pub fn call(&self, arg: T) -> U {
            // TODO has the phantomdata bullshit successfully enforced this guarantee?
            self.send.send(arg).expect("function channel must outlive this one");
            self.recv.recv().expect("function channel must outlive this one")
        }
    }

    impl<T: Send, U: Send, F: Fn(T) -> U> ExternalFunctionListener<T, U, F> {
        /// Returns Err iff ExternalFunction has hung up
        #[must_use]
        pub fn listen_once_timeout(&self, d: Duration) -> Result<(), ()> {
            let input = match self.recv.recv_timeout(d) {
                Ok(i) => i,
                Err(RecvTimeoutError::Timeout) => return Ok(()),
                Err(RecvTimeoutError::Disconnected) => return Err(()),
            };
            self.send.send((self.behavior)(input)).map_err(|_| ())?;
            Ok(())
        }

    }
}

/// Messages to trigger events that can't be contained to the update function
pub enum Extrinsic<S: TuiState> {
    Halt,
    /// for after we temporarily break out of the ratatui environment
    ResetRatatui,
    ResolveAfter(Duration, Box<dyn Send + FnOnce(&S) -> SmallVec<[S::Message; 1]>>),
}

pub trait Message: Sized {
    fn init() -> Option<Self> {
        None
    }
}

pub trait TuiState: Sized {
    type Message: Message;
    type Call: Send;
    type Response: Send;
    type Output;

    // type MessageVec = SmallVec<[Self::Message; 1]>;

    fn handle_message(&mut self, message: Self::Message, external_function: &sync::ExternalFunction<Self::Call, Self::Response>) -> TaskitResult<Option<Extrinsic<Self>>>;
    fn handle_keypresses(&self, event: CEvent) -> SmallVec<[Self::Message; 1]>;
    fn render(&mut self, frame: &mut Frame);
    fn external_function(req: Self::Call) -> Self::Response;
    fn get_output(self) -> Self::Output;

    fn generate_messages(&self, event: Result<CEvent, Box<dyn Send + FnOnce(&Self) -> SmallVec<[Self::Message; 1]>>>) -> SmallVec<[Self::Message; 1]> {
        match event {
            Ok(ev) => self.handle_keypresses(ev),
            Err(f) => f(self),
        }

    }

    #[must_use]
    fn run(mut self) -> TaskitResult<Self::Output> {
        let mut terminal = ratatui::init();

        let mut messages = vec![];

        thread::scope(|s| {
            let (keypress_tx, keypress_rx) = mpsc::channel();
            let (inquire_function, inquire_listener) = sync::function(Self::external_function);
            // let (inquire_request_tx, inquire_request_rx) = mpsc::channel::<InquireRequest>();
            // let (inquire_response_tx, inquire_response_rx) = mpsc::channel::<InquireResponse>();
            {
                let tx = keypress_tx.clone();
                // this thread will eventually exit because when the scope exits, first variables
                // (including senders/receivers) are dropped and *then* we join the thread.
                // if it was the other way around, we'd deadlock ourselves immediately
                s.spawn(move || {
                    loop {
                        let event_available = event::poll(Duration::from_millis(50)).expect("assume that terminal works");
                        if event_available {
                            let event = event::read().expect("just polled that one is real");
                            if let Err(_) = tx.send(Ok(event)) { return };
                        }
                        if inquire_listener.listen_once_timeout(Duration::from_millis(50)).is_err() {
                            return;
                        }
                    }
                });
            };
            let mut halt = false;
            Self::Message::init().map(|m| messages.push(m));
            while !halt {
                terminal.draw(|f| self.render(f)).with(Source::DrawingTui)?;
                let event = keypress_rx.recv().expect("sender outlives all calls to this function");
                messages.extend(self.generate_messages(event));
                for message in mem::take(&mut messages).into_iter() {
                    match self.handle_message(message, &inquire_function)? {
                        Some(Extrinsic::Halt) => {halt = true;},
                        Some(Extrinsic::ResetRatatui) => {terminal.clear().with(Source::DrawingTui)?;},
                        Some(Extrinsic::ResolveAfter(duration, res)) => {
                            let tx = keypress_tx.clone();
                            s.spawn(move || {
                                thread::sleep(duration);
                                let _ = tx.send(Err(res));
                            });
                        },
                        None => {},
                    }
                }
            }
            ratatui::restore();
            Ok(self.get_output())
        })
    }
}

