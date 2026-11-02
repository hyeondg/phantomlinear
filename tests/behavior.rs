use phantomlinear::{Cleanup, Consume, Linear, PhantomLinear, ReScopeDrop, cleanup};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

fn panics(f: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(f)).is_err()
}

#[test]
fn marker_requires_completion() {
    assert!(panics(|| drop(PhantomLinear::new())));
    assert!(!panics(|| PhantomLinear::default().complete()));
}

#[test]
fn enclosing_struct_and_container_are_checked_at_runtime() {
    struct Outer {
        _marker: PhantomLinear,
    }
    assert!(panics(|| drop(Outer {
        _marker: PhantomLinear::new()
    })));
    assert!(panics(|| drop(vec![Linear::new(1)])));
    assert!(!panics(|| drop(None::<Linear<u8>>)));
}

#[test]
fn marker_does_not_double_panic_during_unwind() {
    let result = catch_unwind(|| {
        let _marker = PhantomLinear::new();
        panic!("original");
    });
    assert_eq!(
        result.unwrap_err().downcast_ref::<&str>(),
        Some(&"original")
    );
}

#[test]
fn linear_borrow_and_transfer() {
    let mut x = Linear::new(4);
    *x.get_mut() += 1;
    assert_eq!(*x.get(), 5);
    assert_eq!(x.into_inner(), 5);
}

#[test]
fn linear_map_and_terminal_action() {
    let x = Linear::new(4).map(|x| x.to_string());
    assert_eq!(x.consume(|x| x.len()), 1);
}

#[test]
fn terminal_and_map_panics_propagate() {
    assert!(panics(|| Linear::new(1).consume(|_| panic!("terminal"))));
    assert!(panics(|| {
        let _ = Linear::new(1).map(|_| -> () { panic!("map") });
    }));
}

struct Counted(Rc<Cell<usize>>);
impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn payload_is_destroyed_on_accidental_drop() {
    let drops = Rc::new(Cell::new(0));
    assert!(panics(|| drop(Linear::new(Counted(drops.clone())))));
    assert_eq!(drops.get(), 1);
}

#[test]
fn cleanup_runs_exactly_once() {
    let calls = Cell::new(0);
    let guard = Cleanup::new(5, |x| calls.set(calls.get() + x));
    guard.run();
    assert_eq!(calls.get(), 5);
}

#[test]
fn cleanup_accepts_a_true_fnonce_closure() {
    let captured = String::from("owned");
    Cleanup::new((), move |_| drop(captured)).run();
}

#[test]
fn cleanup_extraction_does_not_invoke_callback() {
    let calls = Cell::new(0);
    let value = Cleanup::new(3, |_| calls.set(1)).into_inner();
    assert_eq!(value, 3);
    assert_eq!(calls.get(), 0);
}

#[test]
fn cleanup_drops_unused_closure_captures() {
    let drops = Rc::new(Cell::new(0));
    let captured = Counted(drops.clone());
    let value = Cleanup::new(7, move |_| drop(captured)).into_inner();
    assert_eq!(value, 7);
    assert_eq!(drops.get(), 1);
}

#[test]
fn cleanup_supports_borrowing_and_mutation() {
    let result = Cell::new(0);
    let mut guard = Cleanup::new(2, |x| result.set(x));
    *guard += 3;
    assert_eq!(*guard, 5);
    drop(guard);
    assert_eq!(result.get(), 5);
}

#[test]
fn cleanup_runs_on_question_mark() {
    fn task(calls: &Cell<usize>) -> Result<(), &'static str> {
        let _guard = Linear::new(1).with_cleanup(|x| calls.set(calls.get() + x));
        Err("failure")?;
        Ok(())
    }
    let calls = Cell::new(0);
    assert_eq!(task(&calls), Err("failure"));
    assert_eq!(calls.get(), 1);
}

#[test]
fn cleanup_runs_on_unwind() {
    let calls = Cell::new(0);
    assert!(panics(|| {
        let _guard = Cleanup::new(1, |x| calls.set(x));
        panic!("original");
    }));
    assert_eq!(calls.get(), 1);
}

#[test]
fn callback_panic_does_not_repeat_callback() {
    let calls = Cell::new(0);
    assert!(panics(|| Cleanup::new((), |_| {
        calls.set(calls.get() + 1);
        panic!("callback");
    })
    .run()));
    assert_eq!(calls.get(), 1);
}

struct Resource {
    marker: PhantomLinear,
    calls: Rc<Cell<usize>>,
}
impl Consume for Resource {
    fn consume(self) {
        let Self { marker, calls } = self;
        marker.complete();
        calls.set(calls.get() + 1);
    }
}
fn resource(calls: &Rc<Cell<usize>>) -> Resource {
    Resource {
        marker: PhantomLinear::named("resource"),
        calls: calls.clone(),
    }
}

#[test]
fn affine_adapter_consumes_on_drop() {
    let calls = Rc::new(Cell::new(0));
    drop(ReScopeDrop::new(resource(&calls)));
    assert_eq!(calls.get(), 1);
}

#[test]
fn affine_adapter_extraction_transfers_responsibility() {
    let calls = Rc::new(Cell::new(0));
    let inner = ReScopeDrop::new(resource(&calls)).into_inner();
    assert_eq!(calls.get(), 0);
    inner.consume();
    assert_eq!(calls.get(), 1);
}

#[test]
fn affine_adapter_consumes_during_unwind() {
    let calls = Rc::new(Cell::new(0));
    assert!(panics(|| {
        let _guard = ReScopeDrop::new(resource(&calls));
        panic!("original");
    }));
    assert_eq!(calls.get(), 1);
}

#[test]
fn leaks_are_an_explicit_limitation() {
    assert!(!panics(|| std::mem::forget(PhantomLinear::new())));
}

#[test]
fn macro_evaluates_arguments_once() {
    let evaluations = Cell::new(0);
    let calls = Cell::new(0);
    let guard = cleanup!(
        {
            evaluations.set(evaluations.get() + 1);
            9
        },
        |x| calls.set(x),
    );
    guard.run();
    assert_eq!(evaluations.get(), 1);
    assert_eq!(calls.get(), 9);
}

#[test]
fn send_and_sync_follow_owned_fields() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PhantomLinear>();
    assert_send_sync::<Linear<String>>();
}
