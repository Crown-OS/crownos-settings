//! Section writes, off the interface's thread and coalesced.
//!
//! [`crownos_config::save`] pretty-prints RON, creates the config directory,
//! writes a temporary file and renames it over the real one. Until now every
//! one of those happened inside the widget callback that changed the value —
//! so dragging the brightness slider rewrote `display.ron` on each frame the
//! pointer moved, on the thread that was supposed to be drawing. A slider is
//! the worst case but not a special one: it is simply the control that emits
//! most often.
//!
//! So a write is queued here instead, and one background thread does it. The
//! thread is deliberately a thread and not a task on xilem's runtime: this is
//! blocking file I/O, which is exactly what an async worker must not do, and
//! nothing here has a reason to be woken by anything but a queued write.
//!
//! ## Coalescing
//!
//! Only the *last* value queued for a section is written, and only after
//! [`DEBOUNCE`] has passed since the first of the run. A drag is therefore one
//! file write per [`DEBOUNCE`] rather than one per frame, and the RON on disk
//! is always the value the user let go on.
//!
//! The cost is that the file trails the interface by up to that long, so
//! [`flush`] exists for the one moment that matters — the window closing,
//! where a write still in the queue would be a setting the user changed and
//! then lost.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SendError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

/// How long a section's write waits for a better one to replace it.
///
/// Long enough to swallow a drag, short enough that letting go of a slider and
/// reaching for the window's close button cannot outrun it.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// A write, with the value it is going to write already owned by it.
type Write = Box<dyn FnOnce() + Send>;

enum Job {
    Save { section: &'static str, write: Write },
    /// Write everything queued now, and say when that is done.
    Flush(mpsc::SyncSender<()>),
}

/// Queues `value` to be written to `section`, replacing anything already
/// queued for it.
///
/// Takes the value rather than a reference because the write happens later, on
/// another thread: a section is a handful of fields, and one clone per edit is
/// nothing next to the serialisation it replaces on the UI thread.
pub fn save<S: Serialize + Send + 'static>(section: &'static str, value: S) {
    let Some(writer) = writer() else {
        return write_now(section, &value);
    };

    let job = Job::Save {
        section,
        write: Box::new(move || write_now(section, &value)),
    };

    // The writer thread is gone. A stutter is better than a lost setting.
    if let Err(SendError(Job::Save { write, .. })) = writer.send(job) {
        write();
    }
}

/// Writes everything still queued, and waits for it to be on disk.
///
/// Call this before the process ends. Everything else is fire and forget.
pub fn flush() {
    let Some(writer) = writer() else {
        return;
    };
    let (done, wait) = mpsc::sync_channel(0);
    if writer.send(Job::Flush(done)).is_ok() {
        let _ = wait.recv();
    }
}

/// The queue, started on first use.
///
/// `None` once and for all if the thread cannot be started, which makes
/// [`save`] fall back to writing inline — a settings window that stutters
/// still beats one that silently stops saving.
fn writer() -> Option<&'static Sender<Job>> {
    static WRITER: OnceLock<Option<Sender<Job>>> = OnceLock::new();

    WRITER
        .get_or_init(|| {
            let (sender, jobs) = mpsc::channel();
            thread::Builder::new()
                .name("crownsettings-config".into())
                .spawn(move || run(&jobs))
                .inspect_err(|err| eprintln!("crownsettings: no config writer thread: {err}"))
                .ok()
                .map(|_| sender)
        })
        .as_ref()
}

fn run(jobs: &Receiver<Job>) {
    let mut pending: HashMap<&'static str, Write> = HashMap::new();
    // When the writes currently queued are due. `None` means none are.
    let mut due: Option<Instant> = None;

    loop {
        let job = match due {
            None => jobs.recv().ok(),
            Some(at) => match jobs.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => {
                    write_all(&mut pending);
                    due = None;
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => None,
            },
        };

        match job {
            Some(Job::Save { section, write }) => {
                // The deadline is set by the *first* write of a run and not
                // pushed back by the ones after it, so a drag that never
                // pauses still reaches the disk every `DEBOUNCE`.
                pending.insert(section, write);
                due.get_or_insert_with(|| Instant::now() + DEBOUNCE);
            }
            Some(Job::Flush(done)) => {
                write_all(&mut pending);
                due = None;
                let _ = done.send(());
            }
            // The app has gone. What is queued is still owed to the user.
            None => return write_all(&mut pending),
        }
    }
}

fn write_all(pending: &mut HashMap<&'static str, Write>) {
    for (_, write) in pending.drain() {
        write();
    }
}

/// Saving is best effort: a read-only config directory should not take the
/// window down, but it should not fail silently either.
fn write_now<S: Serialize>(section: &str, value: &S) {
    if let Err(err) = crownos_config::save(section, value) {
        eprintln!("crownsettings: could not save {section}.ron: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Drives [`run`] on its own thread, with no globals and no files: what is
    /// worth asserting here is which writes happen and how many, not what they
    /// put on disk.
    struct Harness {
        /// `None` once the queue has been closed, which is what tells the
        /// writer thread to finish.
        jobs: Option<Sender<Job>>,
        thread: thread::JoinHandle<()>,
    }

    impl Harness {
        fn new() -> Self {
            let (jobs, receiver) = mpsc::channel();
            Self {
                jobs: Some(jobs),
                thread: thread::spawn(move || run(&receiver)),
            }
        }

        fn jobs(&self) -> &Sender<Job> {
            self.jobs.as_ref().expect("the queue is still open")
        }

        fn save(&self, section: &'static str, count: &Arc<AtomicUsize>) {
            let count = Arc::clone(count);
            let write = Box::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            });
            self.jobs()
                .send(Job::Save { section, write })
                .expect("the writer thread is alive");
        }

        fn flush(&self) {
            let (done, wait) = mpsc::sync_channel(0);
            self.jobs().send(Job::Flush(done)).expect("writer alive");
            wait.recv().expect("flush is acknowledged");
        }

        /// Closes the queue and waits for the thread to finish, the way the
        /// process ending does.
        fn shutdown(mut self) {
            self.jobs = None;
            self.thread
                .join()
                .expect("the writer thread does not panic");
        }
    }

    #[test]
    fn a_run_of_writes_to_one_section_becomes_one_write() {
        let harness = Harness::new();
        let writes = Arc::new(AtomicUsize::new(0));

        // A slider drag, as far as this module can tell.
        for _ in 0..50 {
            harness.save("display", &writes);
        }
        harness.flush();

        assert_eq!(
            writes.load(Ordering::SeqCst),
            1,
            "only the value the user let go on is worth writing"
        );
        harness.shutdown();
    }

    #[test]
    fn sections_do_not_coalesce_into_each_other() {
        let harness = Harness::new();
        let (display, sound) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));

        harness.save("display", &display);
        harness.save("sound", &sound);
        harness.save("display", &display);
        harness.flush();

        assert_eq!(display.load(Ordering::SeqCst), 1);
        assert_eq!(sound.load(Ordering::SeqCst), 1);
        harness.shutdown();
    }

    #[test]
    fn a_write_still_queued_when_the_app_goes_is_made_anyway() {
        let harness = Harness::new();
        let writes = Arc::new(AtomicUsize::new(0));

        // Changed and never flushed — the window closed within the debounce.
        harness.save("display", &writes);
        harness.shutdown();

        assert_eq!(
            writes.load(Ordering::SeqCst),
            1,
            "a setting changed just before quitting must not be lost"
        );
    }

    #[test]
    fn flushing_twice_writes_once() {
        let harness = Harness::new();
        let writes = Arc::new(AtomicUsize::new(0));

        harness.save("display", &writes);
        harness.flush();
        harness.flush();

        assert_eq!(writes.load(Ordering::SeqCst), 1);
        harness.shutdown();
    }
}
