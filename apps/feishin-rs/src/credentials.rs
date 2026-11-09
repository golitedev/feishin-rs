//! Serializes OS credential effects and cancels obsolete authentication jobs.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

enum Task {
    Save {
        session: u64,
        revision: u64,
        profile: ServerProfile,
        password: zeroize::Zeroizing<String>,
    },
    Load {
        session: u64,
        revision: u64,
        profile: ServerProfile,
    },
    Delete {
        session: u64,
        profile: ServerProfile,
    },
}
pub(super) struct CredentialWorker {
    tx: mpsc::SyncSender<Task>,
    revision: Arc<AtomicU64>,
}
impl CredentialWorker {
    pub fn start(tx: mpsc::Sender<Message>, ctx: egui::Context) -> std::io::Result<Self> {
        Self::with_store(tx, ctx, Arc::new(OsCredentialStore))
    }
    fn with_store(
        output: mpsc::Sender<Message>,
        ctx: egui::Context,
        store: Arc<dyn CredentialStore + Send + Sync>,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::sync_channel(128);
        let revision = Arc::new(AtomicU64::new(0));
        let current = revision.clone();
        std::thread::Builder::new()
            .name("feishin-credentials".into())
            .spawn(move || {
                while let Ok(task) = rx.recv() {
                    let (session, result) = match task {
                        Task::Save {
                            session,
                            revision,
                            profile,
                            password,
                        } => {
                            if current.load(Ordering::Acquire) != revision {
                                continue;
                            }
                            (
                                session,
                                workflows::FeatureResult::CredentialSaved(
                                    revision,
                                    profile.clone(),
                                    store.save(&profile, &password).map_err(|e| e.to_string()),
                                ),
                            )
                        }
                        Task::Load {
                            session,
                            revision,
                            profile,
                        } => {
                            if current.load(Ordering::Acquire) != revision {
                                continue;
                            }
                            (
                                session,
                                workflows::FeatureResult::Credentials(
                                    revision,
                                    profile.clone(),
                                    store.load(&profile).map_err(|e| e.to_string()),
                                ),
                            )
                        }
                        Task::Delete { session, profile } => (
                            session,
                            workflows::FeatureResult::Notice(
                                store
                                    .delete(&profile)
                                    .map(|_| {
                                        "Saved password removed from the operating-system store."
                                            .into()
                                    })
                                    .map_err(|e| e.to_string()),
                            ),
                        ),
                    };
                    let _ = output.send(Message::feature(session, result));
                    ctx.request_repaint();
                }
            })?;
        Ok(Self { tx, revision })
    }
    pub fn advance(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }
    pub fn current(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
    pub fn save(
        &self,
        session: u64,
        revision: u64,
        profile: ServerProfile,
        password: zeroize::Zeroizing<String>,
    ) -> Result<(), String> {
        self.tx
            .try_send(Task::Save {
                session,
                revision,
                profile,
                password,
            })
            .map_err(|_| "The credential worker is busy or unavailable".into())
    }
    pub fn load(&self, session: u64, profile: ServerProfile) -> Result<(), String> {
        let revision = self.advance();
        self.tx
            .try_send(Task::Load {
                session,
                revision,
                profile,
            })
            .map_err(|_| "The credential worker is busy or unavailable".into())
    }
    pub fn delete(&self, session: u64, profile: ServerProfile) -> Result<(), String> {
        self.advance();
        self.tx
            .try_send(Task::Delete { session, profile })
            .map_err(|_| "The credential worker is busy or unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, atomic::AtomicUsize};
    struct BlockingStore {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        saves: AtomicUsize,
        deletes: AtomicUsize,
    }
    impl CredentialStore for BlockingStore {
        fn load(
            &self,
            _: &ServerProfile,
        ) -> Result<Option<String>, feishin_platform::credentials::CredentialError> {
            let _ = self.entered.send(());
            let _ = self
                .release
                .lock()
                .map_err(|_| feishin_platform::credentials::CredentialError)?
                .recv_timeout(Duration::from_secs(5));
            Ok(None)
        }
        fn save(
            &self,
            _: &ServerProfile,
            _: &str,
        ) -> Result<(), feishin_platform::credentials::CredentialError> {
            self.saves.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn delete(
            &self,
            _: &ServerProfile,
        ) -> Result<(), feishin_platform::credentials::CredentialError> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[test]
    fn forgetting_a_password_cancels_queued_save_and_runs_delete_last() {
        let (output, rx) = mpsc::channel();
        let (entered, waiting) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let store = Arc::new(BlockingStore {
            entered,
            release: Mutex::new(gate),
            saves: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
        });
        let worker =
            CredentialWorker::with_store(output, egui::Context::default(), store.clone()).unwrap();
        let profile = ServerProfile::default();
        worker.load(0, profile.clone()).unwrap();
        waiting.recv_timeout(Duration::from_secs(2)).unwrap();
        worker
            .save(
                0,
                worker.current(),
                profile.clone(),
                zeroize::Zeroizing::new("synthetic".into()),
            )
            .unwrap();
        worker.delete(0, profile).unwrap();
        release.send(()).unwrap();
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        assert_eq!(store.saves.load(Ordering::SeqCst), 0);
        assert_eq!(store.deletes.load(Ordering::SeqCst), 1);
    }
}
