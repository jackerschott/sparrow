use super::rsync::{rsync, SyncOptions, SyncPayload};
use camino::Utf8Path as Path;
use openssh::{Session, SessionBuilder};

pub struct Connection {
    pub async_runtime: tokio::runtime::Runtime,
    pub session: Session,
}

impl Connection {
    pub fn new(hostname: &str) -> Result<Self, openssh::Error> {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("expected tokio runtime to build successfully");

        let session_builder = SessionBuilder::default();
        let (builder, destination) = session_builder.resolve(hostname);
        let session = async_runtime.block_on(builder.connect(destination))?;

        return Ok(Self {
            async_runtime,
            session,
        });
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

    #[allow(unused)]
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

    pub fn command(&self, program: &str) -> Command {
        Command {
            async_runtime: &self.async_runtime,
            command: self.session.command(program),
        }
    }

    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.async_runtime.block_on(future)
    }
}

pub struct Command<'c> {
    async_runtime: &'c tokio::runtime::Runtime,
    command: openssh::OwningCommand<&'c openssh::Session>,
}

impl<'c> Command<'c> {
    pub fn arg<A: AsRef<str>>(&mut self, arg: A) -> &mut Self {
        self.command.arg(arg);
        self
    }

    pub fn args<I, A>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        self.command.args(args);
        self
    }

    pub fn stdout(&'c mut self, cfg: openssh::Stdio) -> &mut Self {
        self.command.stdout(cfg);
        self
    }

    pub fn stdin(&'c mut self, cfg: openssh::Stdio) -> &mut Self {
        self.command.stdin(cfg);
        self
    }

    #[allow(unused)]
    pub fn stderr(&'c mut self, cfg: openssh::Stdio) -> &mut Self {
        self.command.stderr(cfg);
        self
    }

    pub fn output(&'c mut self) -> Result<std::process::Output, openssh::Error> {
        self.async_runtime.block_on(self.command.output())
    }

    pub fn status(&'c mut self) -> Result<std::process::ExitStatus, openssh::Error> {
        self.async_runtime.block_on(self.command.status())
    }

    pub fn spawn(&'c mut self) -> Result<openssh::Child<&'_ openssh::Session>, openssh::Error> {
        self.async_runtime.block_on(self.command.spawn())
    }
}
