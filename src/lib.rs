extern crate test;

use std::any::Any;
use std::io::timer;
use std::time::duration::Duration;
use std::task::try;


pub enum FutureError{
    TaskFailure(Box<Any+Send>),
    HungUp
}

pub struct Promise<T> {
    sender: Sender<Result<T, FutureError>>
}

impl<T: Send> Promise<T> {

    fn new(tx: Sender<Result<T, FutureError>>) -> Promise<T>{
        Promise{ sender: tx }
    }

    pub fn resolve(self, value: T) -> Result<(), T> {
        match self.sender.send_opt(Ok(value)) {
            Ok(x) => Ok(x),
            Err(Ok(val)) => Err(val),
            _ => unreachable!(),
        }
    }

    fn fail(self, error: FutureError) {
        self.sender.send(Err(error))
    }

}

pub struct Future<T> {
    receiver: Receiver<Result<T, FutureError>>
}

impl<T: Send> Future<T>{

    fn new(rx: Receiver<Result<T, FutureError>>) -> Future<T> {
        Future{ receiver: rx }
    }

    pub fn value(val: T) -> Future<T> {
        let (p, f) = promise::<T>();
        let _ = p.resolve(val);
        f
    }

    pub fn from_fn(func: proc(): Send -> T) -> Future<T> {
        let (p, f) = promise::<T>();
        spawn(proc() {
            match try(func) {
                Ok(val) => {
                    let _ = p.resolve(val);
                },
                Err(err) => {p.fail(TaskFailure(err));},
            };
        });
        f
    }

    pub fn delay(func: proc(): Send -> T, duration: Duration) -> Future<T> {
        Future::from_fn(proc() {
            timer::sleep(duration);
            func()
        })
    }

    pub fn map<B: Send>(self, func: proc(T): Send -> B) -> Future<B> {
        let (p ,f) = promise::<B>();
        self.on_complete(proc(res) {
            match res {
                Ok(val) => {
                    match try(proc() func(val)) {
                        Ok(mapped) => {
                            let _ = p.resolve(mapped);
                        },
                        Err(err) => {p.fail(TaskFailure(err));},
                    };
                },
                Err(err) => p.fail(err),
            };
        });
        f
    }

    pub fn get(self) -> Result<T, FutureError> {
        match self.receiver.recv_opt() {
            Ok(res) => res,
            Err(_) => Err(HungUp),
        }
    }

    pub fn on_complete(self, f: proc(Result<T, FutureError>):Send) {
        spawn(proc(){
            let result = self.get();
            f(result);
        });
    }

}

pub fn promise<T :Send>() -> (Promise<T>, Future<T>) {
    let (tx, rx) = channel();
    (Promise::new(tx), Future::new(rx))
}


#[cfg(test)]
mod tests {
    use super::{promise, Future, HungUp, TaskFailure};
    use std::any::AnyRefExt;
    use std::boxed::BoxAny;
    use std::time::duration::Duration;
    use std::io::timer;


    #[test]
    fn test_future(){
        let (p, f) = promise();
        assert_eq!(p.resolve(123u), Ok(()));
        assert_eq!(f.get().ok(), Some(123u));
    }

    #[test]
    fn test_future_hungup(){
        let (p, f) = promise::<uint>();
        spawn(proc(){
            timer::sleep(Duration::seconds(1));
            p;
        });
        match f.get() {
            Err(HungUp) => (),
            _ => fail!("should not happen"),
        }
    }

    #[test]
    fn test_future_from_fn(){
        let f = Future::from_fn(proc() 123u);
        assert_eq!(f.get().ok(), Some(123u));
    }

    #[test]
    fn test_future_from_fn_fail(){
        let f = Future::from_fn(proc() {
            fail!("ooops");
            123u
        });
        let err = match f.get() {
            Err(TaskFailure(err)) => err,
            _ => fail!("should not happen"),
        };
        assert!(err.is::<&'static str>());
        assert_eq!(*err.downcast::<&'static str>().unwrap(), "ooops");
    }

    #[test]
    fn test_future_delay(){
        let f = Future::delay(proc() 123u, Duration::seconds(3));
        //TODO: test delay
        assert_eq!(f.get().ok(), Some(123u));
    }

    #[test]
    fn test_future_value(){
        let f = Future::value(123u);
        assert_eq!(f.get().ok(), Some(123u));
    }

    #[test]
    fn test_future_on_complete(){
        let (tx, rx) = channel();
        let f = Future::delay(proc() 123u, Duration::seconds(3));
        f.on_complete(proc(x){
            tx.send(x);
        });
        assert_eq!(rx.recv().ok(), Some(123u))
    }

    #[test]
    fn test_future_map(){
        let (tx, rx) = channel();
        let f = Future::value(3u);
        f.map(proc(x) x*x)
         .on_complete(proc(x){
            tx.send(x);
        });
        assert_eq!(rx.recv().ok(), Some(9u));
    }

}
