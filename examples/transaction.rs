use phantomlinear::{Cleanup, PhantomLinear};

// The terminal operations are the only public ways to discharge this marker.
// No Drop impl: terminal methods can destructure Self safely.
struct Transaction {
    marker: PhantomLinear,
    pending: Vec<String>,
}
impl Transaction {
    fn begin() -> Self {
        Self {
            marker: PhantomLinear::named("transaction"),
            pending: vec![],
        }
    }
    fn write(&mut self, value: impl Into<String>) {
        self.pending.push(value.into());
    }
    fn commit(self) -> Vec<String> {
        let Self { marker, pending } = self;
        marker.complete();
        pending
    }
    fn rollback(self) {
        let Self { marker, pending } = self;
        marker.complete();
        drop(pending);
    }
}

fn work(fail: bool) -> Result<Vec<String>, &'static str> {
    let mut tx = Cleanup::new(Transaction::begin(), Transaction::rollback);
    tx.write("first write");
    if fail {
        Err("operation failed")?;
    }
    Ok(tx.into_inner().commit())
}

fn main() {
    assert_eq!(work(false).unwrap(), vec!["first write"]);
    assert_eq!(work(true), Err("operation failed"));
}
