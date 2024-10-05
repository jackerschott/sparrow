use super::rsync::{rsync, SyncOptions, SyncPayload};
use camino::Utf8Path as Path;
use openssh::{Session, SessionBuilder};

pub struct Connection {
    pub session: Session,
}

impl Connection {
    pub async fn new(hostname: &str) -> Self {
        let session_builder = SessionBuilder::default();
        let (builder, destination) = session_builder.resolve(hostname);
        let session = builder
            .connect(destination)
            .await
            .expect(&format!("connection to {} should work", hostname));

        return Self { session };
    }

    fn control_socket_path(&self) -> &Path {
        return Path::from_path(self.session.control_socket())
            .expect("control socket path is not a valid utf8 string");
    }

    pub fn upload(&self, local_path: &Path, remote_path: &Path, options: SyncOptions) {
        rsync(
            SyncPayload::LocalToRemote {
                control_path: self.control_socket_path(),
                sources: &vec![local_path],
                destination: remote_path,
            },
            options,
        )
        .expect("rsync should not fail");
    }

    pub fn download(&self, remote_path: &Path, local_path: &Path, options: SyncOptions) {
        rsync(
            SyncPayload::RemoteToLocal {
                control_path: self.control_socket_path(),
                source: remote_path,
                destination: local_path,
            },
            options,
        )
        .expect("rsync should not fail");
    }
}
