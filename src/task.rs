use crate::registry::{self, RegistryItem};
use anyhow::Result;
use async_trait::async_trait;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};

pub type Routine = Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'static>>;

fn fire_progress_routine(
    self_id: u64,
    mut rx: Receiver<usize>,
    rc: Arc<AtomicBool>,
) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        loop {
            let received = rx.recv().await;
            if let Some(nprog) = received {
                registry::update(self_id, nprog).await?;
            }

            if rc.load(Ordering::SeqCst) {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    })
}

#[async_trait]
pub trait Task {
    fn init_size(&self) -> usize;
    fn body(&self, progress_tx: Sender<usize>) -> Result<Routine>;
    fn kind(&self) -> String;
    fn store_id(&self, id: u64);
    fn id(&self) -> u64;

    async fn fire(&self) -> Result<u64> {
        let size = self.init_size();
        let self_id = registry::get_free_id().await;
        self.store_id(self_id);
        let (tx, rx) = channel(128);
        let routine = self.body(tx)?;

        Ok(registry::push(RegistryItem {
            handle: tokio::spawn(async move {
                let reached = Arc::new(AtomicBool::new(false));
                let rc = reached.clone();

                let progress_routine = fire_progress_routine(self_id, rx, rc);

                routine.await?;
                reached.store(true, Ordering::SeqCst);
                registry::remove(self_id).await?;
                progress_routine.await??;
                Ok(())
            }),
            expected_len: size,
            progress: Arc::new(AtomicUsize::new(0)),
            kind: self.kind(),
        })
        .await?)
    }

    async fn ask(&self) -> Result<Option<(usize, usize, String)>> {
        Ok(registry::ask(self.id()).await?)
    }

    async fn ask_percent(&self) -> Result<usize> {
        Ok(registry::ask_percent(self.id()).await?)
    }

    async fn ask_percent_float(&self) -> Result<f64> {
        Ok(registry::ask_percent_float(self.id()).await?)
    }

    async fn cancel(&self) -> Result<()> {
        Ok(registry::cancel(self.id()).await?)
    }
}
