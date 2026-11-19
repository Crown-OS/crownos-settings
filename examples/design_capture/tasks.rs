//! `cx.spawn` futures run on a tokio thread, as the app's runtime runs them,
//! with each output handed back to the UI thread by [`Pump::drain`].

use std::any::Any;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

use anyhow::{Context, Result};
use tokio::runtime::Handle;
use tokio::task::AbortHandle;
use view::{TaskExecutor, TaskFuture, TaskId};

type Finished = (TaskId, Box<dyn Any + Send>);

struct ThreadedTasks {
    runtime: Handle,
    finished: Sender<Finished>,
    running: HashMap<TaskId, AbortHandle>,
}

impl TaskExecutor for ThreadedTasks {
    fn spawn(&mut self, id: TaskId, future: TaskFuture) {
        let finished = self.finished.clone();
        let task = self.runtime.spawn(async move {
            let _ = finished.send((id, future.await));
        });
        self.running.retain(|_, task| !task.is_finished());
        self.running.insert(id, task.abort_handle());
    }

    fn cancel(&mut self, id: TaskId) {
        if let Some(task) = self.running.remove(&id) {
            task.abort();
        }
    }
}

/// The receiving end of one window's finished tasks.
pub struct Pump {
    finished: Receiver<Finished>,
}

impl Pump {
    /// Every output that has arrived since the last drain.
    pub fn drain(&self) -> impl Iterator<Item = Finished> + '_ {
        self.finished.try_iter()
    }
}

/// An executor for one window, and the pump its outputs arrive on.
pub fn threaded() -> Result<(Box<dyn TaskExecutor>, Pump)> {
    let (finished, receiver) = channel();
    let executor = ThreadedTasks {
        runtime: task_runtime()?.clone(),
        finished,
        running: HashMap::new(),
    };
    Ok((Box::new(executor), Pump { finished: receiver }))
}

fn task_runtime() -> Result<&'static Handle> {
    static RUNTIME: OnceLock<Handle> = OnceLock::new();
    if let Some(handle) = RUNTIME.get() {
        return Ok(handle);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a task runtime")?;
    let handle = runtime.handle().clone();
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));
    Ok(RUNTIME.get_or_init(|| handle))
}
