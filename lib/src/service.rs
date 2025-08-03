use std::fs::File;
use std::io::{BufRead, BufReader, Read, Stdout, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process;
use std::process::{ChildStdout, Stdio};
use std::str::FromStr;
use std::time::Duration;

use crate::model::ServiceModel;
use crate::time::Instant;
use crate::{Error, Result};

/// Describes all possible types of managed services.
///
/// # Portability problems
///
/// Some services may be unable to run on some architectures. At the same time
/// they may be required for processing entities of certain types.
///
/// One solution to this problem is tying specific services to certain entity
/// types. As the runtime understands what services can be run on what
/// machines, it can prevent spawning of certain types of entities on specific
/// nodes.
///
/// Another solution is writing "non-local-only" services, that also mutate
/// remote entities in addition to locally stored ones.
pub enum ManagedServiceType {
    /// Expected to exist on all entity-handling nodes
    Universal,
    /// Expected to exist on all nodes where at least one entity of certain
    /// type currently lives, entity type is specified as list of components
    EntityTypeBound(Vec<String>),
    /// Expected to exist only on the leader server
    LeaderBound,
    /// Expected to exist on specific workers by id
    WorkersBound(Vec<u32>),
    /// Expected to exist on a number of most performant machines
    MostPerformant(u32),
}

impl ManagedServiceType {
    pub fn new(s: &String, args: Option<&String>) -> Result<Self> {
        let s_ = match s.to_lowercase().as_str() {
            "universal" => Self::Universal,
            "leaderbound" | "leader_bound" => Self::LeaderBound,
            "entitytypebound" | "entitytype" | "entity_type" => match args {
                Some(a) => {
                    let string_args = a.split(',').map(|s| s.to_string()).collect::<Vec<String>>();
                    Self::EntityTypeBound(string_args)
                }
                None => {
                    return Err(Error::Other(format!(
                        "service type {} requires additional argument",
                        s
                    )))
                }
            },
            "workersbound" | "workers_bound" | "workers" => match args {
                Some(a) => {
                    let mut ids = Vec::new();
                    for str in a.split(',') {
                        ids.push(str.parse().unwrap());
                    }
                    Self::WorkersBound(ids)
                }
                None => {
                    return Err(Error::Other(format!(
                        "service type {} requires additional argument",
                        s
                    )))
                }
            },
            "mostperformant" | "most_performant" | "performance" => match args {
                Some(a) => Self::MostPerformant(a.parse().unwrap()),
                None => {
                    return Err(Error::Other(format!(
                        "service type {} requires additional argument",
                        s
                    )))
                }
            },
            _ => {
                return Err(Error::Other(format!(
                    "failed parsing service type from string: {}",
                    s
                )))
            }
        };
        Ok(s_)
    }
}

/// Managed service connected as a client to local or remote server.
///
/// # Managed service
///
/// Service is monitored by it's parent process, who's collecting output and
/// metrics, and checking if the service is alive. If a service process
/// crashes, there will be an attempt to restart it.
///
/// # Services as clients
///
/// Services are handled on the server level because, as clients, they require
/// a connection to server to function properly.
pub struct Service {
    pub type_: ManagedServiceType,
    /// Project-wide unique name of the service
    pub name: String,
    /// Path to service binary
    pub bin_path: PathBuf,
    args: Vec<String>,

    /// Handle to the child process
    pub handle: std::process::Child,

    /// Spawn time of last service instance
    started_at: Instant,
    /// Address of the service client
    address: Option<SocketAddr>,
    server_address: SocketAddr,

    /// Cumulative log for stdout
    pub std_out_log: String,

    pub output_path: Option<PathBuf>,
}

// TODO support compiling rust services from path to src using cargo
// impl Service {
//     pub fn start_from_model(model: ServiceModel, server_addr: String) -> Result<Self> {
//         let bin_path = if let Some(executable_path) = &model.executable {
//             executable_path
//         } else if let Some(src_path) = model.project {
//             unimplemented!("compiling rust services from path to src")
//         } else {
//             panic!("service must provide path to executable or to compilable project")
//         };
//
//         let mut cmd = process::Command::new(model.executable.as_ref().unwrap());
//         // cmd.args(&["--server_address", &server_addr]);
//         cmd.args(&model.args);
//         cmd.stdout(Stdio::inherit());
//         let started_at = Instant::now();
//         let mut child = cmd.spawn()?;
//
//         let service = Self {
//             type_: if let Some(t) = model.type_ {
//                 ManagedServiceType::new(&t, model.type_args.as_ref())?
//             } else {
//                 ManagedServiceType::Universal
//             },
//             name: model.name.clone(),
//             bin_path: bin_path.to_path_buf(),
//             args: model.args.clone(),
//             handle: child,
//             started_at,
//             address: None,
//             server_address: SocketAddr::from_str(&server_addr).unwrap(),
//             std_out_log: "".to_string(),
//             output_path: model.output.map(|o| PathBuf::from_str(&o).unwrap()),
//         };
//
//         Ok(service)
//     }
//
//     pub fn get_uptime(&self) -> Duration {
//         Instant::now() - self.started_at
//     }
//
//     pub fn monitor(&mut self) {
//         // let mut buf = [0; 100];
//         // if let Ok(n) = &self.stdout.as_mut().unwrap().read(&mut buf) {
//
//         // {
//         //     let mut stdout = self.handle.stdout.as_mut().unwrap();
//         //     let stdout_reader = BufReader::new(&mut stdout);
//         //     let stdout_lines = stdout_reader.lines().;
//         //     for line in stdout_lines {
//         //         println!("Read: {:?}", line);
//         //     }
//         // }
//         // println!("finished");
//
//         // if let Ok(n) = self.stdout.read(&mut buf) {
//         //     println!("inside");
//         //     trace!("read number of bytes from service stdout: {}", n);
//         //     let new_output = String::from_utf8_lossy(&buf[0..n]);
//         //     self.std_out_log.push_str(&new_output);
//         //     println!("{}", new_output);
//         //     // if let Some(output_path) = &self.output_path {
//         //     //     let mut file = File::open(output_path).unwrap();
//         //     //     file.write_all(&buf);
//         //     // }
//         // }
//
//         // check if the service is running
//         if let Ok(status) = self.handle.try_wait() {
//             if let Some(s) = status {
//                 warn!(
//                     "service \"{}\" found dead with exit status: {}, attempting to restart...",
//                     self.name, s
//                 );
//                 self.restart(false);
//             }
//             return;
//         }
//     }
//
//     /// Restarts a service by killing it and starting a new instance.
//     pub fn restart(&mut self, kill: bool) -> Result<()> {
//         if kill {
//             // self.handle.kill()?;
//             self.stop()?;
//         }
//         self.handle = process::Command::new(&self.bin_path)
//             // .arg(self.server_address.to_string())
//             .args(&self.args)
//             .spawn()?;
//         self.started_at = Instant::now();
//
//         Ok(())
//     }
//
//     /// Stops a service by killing it.
//     // TODO stop services gracefully
//     pub fn stop(&mut self) -> Result<()> {
//         self.handle.kill()?;
//         Ok(())
//     }
// }
