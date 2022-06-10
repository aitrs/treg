use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tokio::{sync::RwLock, task::JoinHandle};

pub struct RegistryItem {
    pub handle: JoinHandle<Result<()>>,
    pub expected_len: usize,
    pub progress: Arc<AtomicUsize>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDescriptor {
    pub len: usize,
    pub progress: usize,
    pub kind: String,
    pub id: u64,
}
type RegistryCore = BTreeMap<u64, RegistryItem>;

struct Registry(RegistryCore);

impl Registry {
    pub(super) fn new() -> Self {
        Registry(BTreeMap::new())
    }

    pub(super) fn get_free_id(&self) -> u64 {
        let mut values = self.0.iter().map(|(id, _)| *id).collect::<Vec<u64>>();

        values.sort_by(|a, b| b.cmp(a));
        if !values.is_empty() {
            values[0] + 1
        } else {
            1
        }
    }

    pub(super) fn push(&mut self, item: RegistryItem) -> u64 {
        let id = self.get_free_id();
        self.0.insert(id, item);
        id
    }

    pub(super) fn ask(&self, id: u64) -> Option<(usize, usize, String)> {
        self.0.get(&id).map(|task| {
            (
                task.progress.load(Ordering::SeqCst),
                task.expected_len,
                task.kind.to_string(),
            )
        })
    }

    pub(super) fn ask_percent(&self, id: u64) -> usize {
        self.0
            .get(&id)
            .map(|task| task.progress.load(Ordering::SeqCst) * 100usize / task.expected_len)
            .unwrap_or(0usize)
    }

    pub(super) fn ask_percent_float(&self, id: u64) -> f64 {
        self.0
            .get(&id)
            .map(|task| {
                let float_size = task.expected_len as f64;
                let float_progress = task.progress.load(Ordering::SeqCst) as f64;

                float_progress * 100f64 / float_size
            })
            .unwrap_or(0f64)
    }

    pub(super) fn list_all(&self) -> Vec<TaskDescriptor> {
        self.0
            .iter()
            .map(|(id, item)| TaskDescriptor {
                id: *id,
                len: item.expected_len,
                progress: item.progress.load(Ordering::SeqCst),
                kind: item.kind.to_string(),
            })
            .collect()
    }

    pub(super) fn update(&self, id: u64, progress: usize) {
        if let Some(task) = self.0.get(&id) {
            task.progress.store(progress, Ordering::SeqCst);
        }
    }

    pub(super) fn cancel(&mut self, id: u64) {
        if let Some(task) = self.0.get(&id) {
            task.handle.abort();
            self.0.remove(&id);
        }
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.0.remove(&id);
    }

    pub(super) fn count(&self) -> usize {
        self.0.len()
    }
}

static mut REGISTRY: Option<RwLock<Registry>> = None;

pub(crate) async fn push(item: RegistryItem) -> Result<u64> {
    if unsafe { REGISTRY.is_none() } {
        unsafe {
            REGISTRY = Some(RwLock::new(Registry::new()));
        }
    }

    if let Some(lock) = unsafe { &REGISTRY } {
        let mut reg = lock.write().await;

        Ok(reg.push(item))
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn ask(id: u64) -> Result<Option<(usize, usize, String)>> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let reg = lock.read().await;

        Ok(reg.ask(id))
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn ask_percent(id: u64) -> Result<usize> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let req = lock.read().await;

        Ok(req.ask_percent(id))
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn ask_percent_float(id: u64) -> Result<f64> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let req = lock.read().await;

        Ok(req.ask_percent_float(id))
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn list_all() -> Vec<TaskDescriptor> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let reg = lock.read().await;

        reg.list_all()
    } else {
        vec![]
    }
}

pub(crate) async fn cancel(id: u64) -> Result<()> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let mut reg = lock.write().await;
        reg.cancel(id);
        Ok(())
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn update(id: u64, progress: usize) -> Result<()> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let req = lock.read().await;
        req.update(id, progress);
        Ok(())
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn remove(id: u64) -> Result<()> {
    if let Some(lock) = unsafe { &REGISTRY } {
        let mut reg = lock.write().await;
        reg.remove(id);
        Ok(())
    } else {
        Err(anyhow!("Uninitialized Registry"))
    }
}

pub(crate) async fn get_free_id() -> u64 {
    if let Some(lock) = unsafe { &REGISTRY } {
        let req = lock.write().await;
        req.get_free_id()
    } else {
        1
    }
}

pub(crate) async fn count() -> usize {
    if let Some(lock) = unsafe { &REGISTRY } {
        let reg = lock.read().await;
        reg.count()
    } else {
        0
    }
}

#[cfg(test)]
mod registry_tests {
    use std::sync::{atomic::AtomicUsize, Arc};

    use tokio::time::Instant;

    use crate::registry::RegistryItem;

    use super::Registry;

    #[tokio::test]
    async fn get_free_id_test() {
        let mut reg = Registry::new();

        assert_eq!(reg.get_free_id(), 1);
        reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1usize,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        assert_eq!(reg.get_free_id(), 2);
    }

    #[tokio::test]
    async fn push_test() {
        let mut reg = Registry::new();
        reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });

        assert_eq!(reg.count(), 2);
    }

    #[tokio::test]
    async fn ask_test() {
        let mut reg = Registry::new();
        assert!(reg.ask(1).is_none());
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        let a = reg.ask(id);

        assert!(a.is_some());
        if let Some(ask) = a {
            assert_eq!(ask.0, 0);
            assert_eq!(ask.1, 1);
            assert_eq!(ask.2, "TEST".to_string());
        } else {
            assert!(false);
        }
    }

    #[tokio::test]
    async fn ask_percent_test() {
        let mut reg = Registry::new();
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 2,
            progress: Arc::new(AtomicUsize::new(1)),
            kind: "TEST".to_string(),
        });
        let a = reg.ask_percent(id);
        assert_eq!(a, 50usize);
    }

    #[tokio::test]
    async fn ask_percent_float_test() {
        let mut reg = Registry::new();
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 2,
            progress: Arc::new(AtomicUsize::new(1)),
            kind: "TEST".to_string(),
        });
        let a = reg.ask_percent_float(id);
        assert_eq!(a, 50f64);
    }

    #[tokio::test]
    async fn list_all_test() {
        let mut reg = Registry::new();
        reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 2,
            progress: Arc::new(AtomicUsize::new(1)),
            kind: "TEST2".to_string(),
        });
        let list = reg.list_all();
        assert_eq!(list.len(), 2usize);
        assert_eq!(list[0].id, 1);
        assert_eq!(list[0].len, 1);
        assert_eq!(list[0].progress, 0);
        assert_eq!(list[0].kind, "TEST".to_string());
        assert_eq!(list[1].id, 2);
        assert_eq!(list[1].len, 2);
        assert_eq!(list[1].progress, 1);
        assert_eq!(list[1].kind, "TEST2".to_string());
    }

    #[tokio::test]
    async fn update_test() {
        let mut reg = Registry::new();
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        reg.update(id, 1);
        let a = reg.ask(id);

        assert!(a.is_some());
        if let Some(ask) = a {
            assert_eq!(ask.0, 1);
        } else {
            assert!(false)
        }
    }

    #[tokio::test]
    async fn cancel_test() {
        let mut reg = Registry::new();
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                let now = Instant::now();
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    if now.elapsed().ge(&tokio::time::Duration::from_secs(1)) {
                        break;
                    }
                }
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        reg.cancel(id);
        assert_eq!(reg.count(), 0);
    }

    #[tokio::test]
    async fn remove_test() {
        let mut reg = Registry::new();
        let id = reg.push(RegistryItem {
            handle: tokio::spawn(async move {
                let now = Instant::now();
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    if now.elapsed().ge(&tokio::time::Duration::from_secs(1)) {
                        break;
                    }
                }
                Ok(())
            }),
            expected_len: 1,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: "TEST".to_string(),
        });
        assert_eq!(reg.count(), 1);
        reg.remove(id);
        assert_eq!(reg.count(), 0);
    }
}
