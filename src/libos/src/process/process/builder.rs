use super::super::task::Task;
use super::super::thread::{ThreadBuilder, ThreadId, ThreadName};
use super::super::{
    FileTableRef, ForcedExitStatus, FsViewRef, ProcessRef, ProcessVMRef, ResourceLimitsRef,
    SchedAgentRef,
};
use super::{Process, ProcessInner};
use crate::fs::FileMode;
use crate::prelude::*;
use crate::signal::{SigDispositions, SigQueues, SigSet};

#[derive(Debug)]
pub struct ProcessBuilder {
    tid: Option<ThreadId>,
    thread_builder: Option<ThreadBuilder>,
    // Mandatory fields
    vm: Option<ProcessVMRef>,
    // Optional fields, which have reasonable default values
    exec_path: Option<String>,
    umask: Option<FileMode>,
    parent: Option<ProcessRef>,
    no_parent: bool,
    sig_dispositions: Option<SigDispositions>,
}

impl ProcessBuilder {
    pub fn new() -> Self {
        let thread_builder = ThreadBuilder::new();
        Self {
            tid: None,
            thread_builder: Some(thread_builder),
            vm: None,
            exec_path: None,
            umask: None,
            parent: None,
            no_parent: false,
            sig_dispositions: None,
        }
    }

    pub fn tid(mut self, tid: ThreadId) -> Self {
        self.tid = Some(tid);
        self
    }

    pub fn exec_path(mut self, exec_path: &str) -> Self {
        self.exec_path = Some(exec_path.to_string());
        self
    }

    pub fn umask(mut self, umask: FileMode) -> Self {
        self.umask = Some(umask);
        self
    }

    pub fn parent(mut self, parent: ProcessRef) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn no_parent(mut self, no_parent: bool) -> Self {
        self.no_parent = no_parent;
        self
    }

    pub fn sig_dispositions(mut self, sig_dispositions: SigDispositions) -> Self {
        self.sig_dispositions = Some(sig_dispositions);
        self
    }

    pub fn task(mut self, task: Task) -> Self {
        self.thread_builder(|tb| tb.task(task))
    }

    pub fn sched(mut self, sched: SchedAgentRef) -> Self {
        self.thread_builder(|tb| tb.sched(sched))
    }

    pub fn vm(mut self, vm: ProcessVMRef) -> Self {
        self.thread_builder(|tb| tb.vm(vm))
    }

    pub fn fs(mut self, fs: FsViewRef) -> Self {
        self.thread_builder(|tb| tb.fs(fs))
    }

    pub fn files(mut self, files: FileTableRef) -> Self {
        self.thread_builder(|tb| tb.files(files))
    }

    pub fn sig_mask(mut self, sig_mask: SigSet) -> Self {
        self.thread_builder(|tb| tb.sig_mask(sig_mask))
    }

    pub fn rlimits(mut self, rlimits: ResourceLimitsRef) -> Self {
        self.thread_builder(|tb| tb.rlimits(rlimits))
    }

    pub fn name(mut self, name: ThreadName) -> Self {
        self.thread_builder(|tb| tb.name(name))
    }

    pub fn build(mut self) -> Result<ProcessRef> {
        // Process's pid == Main thread's tid
        let tid = self.tid.take().unwrap_or_else(|| ThreadId::new());
        let pid = tid.as_u32() as pid_t;

        // Check whether parent is given as expected
        if self.no_parent != self.parent.is_none() {
            return_errno!(
                EINVAL,
                "parent and no_parent config contradicts with one another"
            );
        }

        // Build a new process
        let new_process = {
            let exec_path = self.exec_path.take().unwrap_or_default();
            let umask = RwLock::new(self.umask.unwrap_or(FileMode::default_umask()));
            let parent = self.parent.take().map(|parent| RwLock::new(parent));
            let inner = SgxMutex::new(ProcessInner::new());
            let sig_dispositions = RwLock::new(self.sig_dispositions.unwrap_or_default());
            let sig_queues = RwLock::new(SigQueues::new());
            let forced_exit_status = ForcedExitStatus::new();
            Arc::new(Process {
                pid,
                exec_path,
                umask,
                parent,
                inner,
                sig_dispositions,
                sig_queues,
                forced_exit_status,
            })
        };

        // Build the main thread of the new process
        let mut self_ = self.thread_builder(|tb| tb.tid(tid).process(new_process.clone()));
        let main_thread = self_.thread_builder.take().unwrap().build()?;

        // Associate the new process with its parent
        if !self_.no_parent {
            new_process
                .parent()
                .inner()
                .children_mut()
                .unwrap()
                .push(new_process.clone());
        }

        Ok(new_process)
    }

    fn thread_builder<F>(mut self, f: F) -> Self
    where
        F: FnOnce(ThreadBuilder) -> ThreadBuilder,
    {
        let thread_builder = self.thread_builder.take().unwrap();
        self.thread_builder = Some(f(thread_builder));
        self
    }
}
