#![feature(unboxed_closures)]
extern crate test;

use std::any::Any;
use std::io::timer;
use std::time::duration::Duration;
use std::sync::mpsc::{
    Select,
    Sender,
    SendError,
    Receiver,
    channel
};
use std::collections::HashMap;
use std::thread::Thread;

pub enum FutureError{
    TaskFailure(Box<Any+Send>),
    HungUp
}

/// A promise is used to set the value of the associated Future
pub struct Promise<T> {
    sender: Sender<Result<T, FutureError>>
}

impl<T: Send> Promise<T> {

    fn new(tx: Sender<Result<T, FutureError>>) -> Promise<T>{
        Promise{ sender: tx }
    }

    /// Completes the associated Future with value;
    pub fn resolve(self, value: T) -> Result<(), T> {
        match self.sender.send(Ok(value)) {
            Ok(x) => Ok(x),
            Err(SendError(Ok(val))) => Err(val),
            _ => unreachable!(),
        }
    }

    fn send(self, value: Result<T, FutureError>){
        self.sender.send(value);
    }

    fn fail(self, error: FutureError) {
        self.sender.send(Err(error));
    }

}

/// A future represents a value that is not yet available
pub struct Future<T> {
    receiver: Receiver<Result<T, FutureError>>
}


impl<T: Send> Future<T>{

    fn new(rx: Receiver<Result<T, FutureError>>) -> Future<T> {
        Future{ receiver: rx }
    }

    pub fn first_of(futures: Vec<Future<T>>) -> Future<T> {
        let (p, f) = promise::<T>();
        Thread::spawn(move || {
            let select = Select::new();
            let mut handles = HashMap::new();
            for future in futures.iter() {
                let handle = select.handle(&future.receiver);
                let id = handle.id();
                handles.insert(handle.id(), handle);
                let h = handles.get_mut(&id).unwrap();
                unsafe {
                    h.add();
                }
            }
            {
                let first = handles.get_mut(&select.wait()).unwrap();
                p.send(
                    match first.recv() {
                        Ok(res) => res,
                        Err(_) => Err(FutureError::HungUp),
                    }
                );
            }

            for (_, handle) in handles.iter_mut() {
                unsafe {
                    handle.remove();
                }
            }
        });
        f
    }

    // Warning this function is pretty ugly mostly due to the move restrictions on handle for add
    // and remove. It needs to be rewritten at some point.
    pub fn all(futures: Vec<Future<T>>) -> Future<Vec<T>> {
        let (p, f) = promise::<Vec<T>>();
        Thread::spawn(move || {
            let select = Select::new();
            let mut handles = HashMap::new();
            for (i, future) in futures.iter().enumerate() {
                let handle = select.handle(&future.receiver);
                let id = handle.id();
                handles.insert(handle.id(), (i, handle));
                let &mut (_, ref mut handle) = handles.get_mut(&id).unwrap();
                unsafe {
                    handle.add();
                }
            }

            let mut results: Vec<Option<T>> = futures.iter().map(|_| None).collect();
            let mut error: Option<FutureError> = None;

            for _ in range(0, futures.len()) {
                let id = select.wait();
                {
                    let &mut (i, ref mut handle) = handles.get_mut(&id).unwrap();
                    match handle.recv() {
                        Ok(Ok(value)) => {
                            *results.get_mut(i).unwrap() = Some(value);
                        },
                        Ok(Err(err)) => {
                            error = Some(err);
                            break;
                        },
                        Err(_) => {
                            error = Some(FutureError::HungUp);
                            break;
                        },
                    }
                    unsafe{
                        handle.remove();
                    }
                }
                handles.remove(&id);
            }

            for (_, &mut (_, ref mut handle)) in handles.iter_mut() {
                unsafe {
                    handle.remove();
                }
            }

            match error {
                Some(err) => p.fail(err),
                None => {
                    let _ = p.resolve(results.into_iter().map(|v| v.unwrap()).collect());
                }
            }
        });
        f
    }

    /// Creates a Future that completes with val.
    pub fn value(val: T) -> Future<T> {
        let (p, f) = promise::<T>();
        let _ = p.resolve(val);
        f
    }

    /// Creates a Future that resolves with the return value of func,
    /// If func fails the failure is propagated through TaskFailure.
    pub fn from_fn<F: FnOnce<(), T> + Send>(func: F) -> Future<T> {
        let (p, f) = promise::<T>();
        Thread::spawn(move ||  {
            let result = Thread::scoped(move || func()).join();
            match result {
                Ok(val) => {
                    let _ = p.resolve(val);
                },
                Err(err) => {p.fail(FutureError::TaskFailure(err));},
            };
        });
        f
    }

    /// Creates a Future just like from_fn that completes after a delay of duration.
    pub fn delay<F: FnOnce<(), T>+Send>(func: F, duration: Duration) -> Future<T> {
        Future::from_fn(move || {
            timer::sleep(duration);
            func()
        })
    }

    /// If this Future completes with a value the new Future completes with func(value).
    /// If thie Future completes with an errorthe new Future completes with the same error.
    pub fn map<B: Send, F: FnOnce<(T,), B>+Send>(self, func: F) -> Future<B> {
        let (p ,f) = promise::<B>();
        self.on_result(move |res| {
            match res {
                Ok(val) => {
                    let result = Thread::scoped(move || func(val)).join();
                    match result {
                        Ok(mapped) => {
                            let _ = p.resolve(mapped);
                        },
                        Err(err) => {p.fail(FutureError::TaskFailure(err));},
                    };
                },
                Err(err) => p.fail(err),
            };
        });
        f
    }

    /// Synchronously waits for the result of the Future and returns it.
    pub fn get(self) -> Result<T, FutureError> {
        match self.receiver.recv() {
            Ok(res) => res,
            Err(_) => Err(FutureError::HungUp),
        }
    }

    /// Registers a function f that is called with the result of the Future.
    /// This function does not block.
    pub fn on_result<F: FnOnce<(Result<T, FutureError>,), ()>+Send>(self, f: F) {
        Thread::spawn(move || {
            let result = self.get();
            f(result);
        });
    }

    /// Registers a function f that is called if the Future completes with a value.
    /// This function does not block.
    pub fn on_success<F: FnOnce<(T,), ()>+Send>(self, f: F) {
        Thread::spawn(move || {
            match self.get() {
                Ok(value) => f(value),
                _ => (),
            }
        });
    }

    /// Registers a function f that is called if the Future completes with an error.
    /// This function does not block.
    pub fn on_failure<F: FnOnce<(FutureError,), ()>+Send>(self, f: F) {
        Thread::spawn(move || {
            match self.get() {
                Err(err) => f(err),
                _ => () ,
            }
        });
    }

    /// Registers a function f that is called if the Future completes with a value.
    /// This function does not block.
    pub fn on_complete<S: FnOnce<(T,),()>+Send, F: FnOnce<(FutureError,),()>+Send>(self, success: S, failure: F) {
        Thread::spawn(move || {
            match self.get() {
                Ok(value) => success(value),
                Err(err) => failure(err),
            }
        });
    }
}

/// Creates a Future and the associated Promise to complete it.
pub fn promise<T :Send>() -> (Promise<T>, Future<T>) {
    let (tx, rx) = channel();
    (Promise::new(tx), Future::new(rx))
}


#[cfg(test)]
mod tests {
    use super::{promise, Future, FutureError};
    use std::boxed::BoxAny;
    use std::time::duration::Duration;
    use std::io::timer;
    use std::sync::mpsc::{
        channel
    };
    use std::thread::Thread;

    #[test]
    fn test_future(){
        let (p, f) = promise();
        assert_eq!(p.resolve(123us), Ok(()));
        assert_eq!(f.get().ok(), Some(123us));
    }

    #[test]
    fn test_future_hungup(){
        let (p, f) = promise::<usize>();
        Thread::spawn(move || {
            timer::sleep(Duration::seconds(1));
            p;
        });
        match f.get() {
            Err(FutureError::HungUp) => (),
            _ => panic!("should not happen"),
        }
    }

    #[test]
    fn test_future_from_fn(){
        let f = Future::from_fn(move || 123us);
        assert_eq!(f.get().ok(), Some(123us));
    }

    #[test]
    fn test_future_from_fn_fail(){
        let f = Future::from_fn(move ||  {
            panic!("ooops");
            123us
        });
        let err = match f.get() {
            Err(FutureError::TaskFailure(err)) => err,
            _ => panic!("should not happen"),
        };
        assert!(err.is::<&'static str>());
        assert_eq!(*err.downcast::<&'static str>().unwrap(), "ooops");
    }

    #[test]
    fn test_future_delay(){
        let f = Future::delay(move ||  123us, Duration::seconds(3));
        //TODO: test delay
        assert_eq!(f.get().ok(), Some(123us));
    }

    #[test]
    fn test_future_first_of(){
        let f1 = Future::delay(move || "slow", Duration::seconds(3));
        let f2 = Future::from_fn(move || "fast");
        let f3 = Future::first_of(vec![f1,f2]);
        assert_eq!(f3.get().ok(), Some("fast"));
    }

    #[test]
    fn test_future_all_failure(){
        let f1 = Future::delay(move || "slow", Duration::seconds(3));
        let f2 = Future::delay(move || panic!("medium"), Duration::seconds(1));
        let f3 = Future::from_fn(move || "fast");
        let f4 = Future::all(vec![f1,f2,f3]);
        let err = match f4.get() {
            Err(FutureError::TaskFailure(err)) => err,
            _ => panic!("should not happen"),
        };
        assert_eq!(*err.downcast::<&'static str>().unwrap(), "medium");
    }

    #[test]
    fn test_future_all_success(){
        let f1 = Future::delay(move || "slow", Duration::seconds(3));
        let f2 = Future::delay(move || "medium", Duration::seconds(1));
        let f3 = Future::from_fn(move || "fast");
        let f4 = Future::all(vec![f1,f2,f3]);
        assert_eq!(f4.get().ok().unwrap(), vec!["slow", "medium", "fast"]);
    }

    #[test]
    fn test_future_value(){
        let f = Future::value(123us);
        assert_eq!(f.get().ok(), Some(123us));
    }

    #[test]
    fn test_future_on_result(){
        let (tx, rx) = channel();
        let f = Future::delay(move || 123us, Duration::seconds(1));
        f.on_result(move |x| {
            tx.send(x);
        });
        assert_eq!(rx.recv().ok().unwrap().ok().unwrap(), 123us)
    }

    #[test]
    fn test_future_on_success(){
        let (tx, rx) = channel();
        let f = Future::delay(move || 123us, Duration::seconds(1));
        f.on_success(move |x| {
            tx.send(x);
        });
        assert_eq!(rx.recv().ok().unwrap(), 123us)
    }

    #[test]
    fn test_future_map(){
        let (tx, rx) = channel();
        let f = Future::value(3us);
        f.map(move |x| x*x)
         .on_success(move |x| {
            tx.send(x);
        });
        assert_eq!(rx.recv().ok().unwrap(), 9us);
    }

}
