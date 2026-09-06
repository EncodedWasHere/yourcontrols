#![allow(non_snake_case)]

// Console/headless entry point for YourControls.
// Reuses the existing simulator, networking, sync and definition modules,
// but replaces the legacy web-view frontend with terminal output.

#[path = "clientmanager.rs"]
mod clientmanager;
#[path = "corrector.rs"]
mod corrector;
#[path = "definitions.rs"]
mod definitions;
#[path = "emulator.rs"]
mod emulator;
#[path = "paths.rs"]
mod paths;
#[path = "program.rs"]
mod program;
#[path = "simconfig.rs"]
mod simconfig;
#[path = "sync.rs"]
mod sync;
#[path = "syncdefs.rs"]
mod syncdefs;
#[path = "update.rs"]
mod update;
#[path = "util.rs"]
mod util;
#[path = "varreader.rs"]
mod varreader;

mod app {
    use crossbeam_channel::{unbounded, Receiver, TryRecvError};
    use laminar::Metrics;
    use serde::{Deserialize, Serialize};
    use std::net::IpAddr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use crate::cli::{startup_plan, StartupMode};
    use crate::simconfig;

    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[serde(rename_all = "camelCase")]
    pub enum ConnectionMethod {
        Direct,
        Relay,
        CloudServer,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(tag = "type", rename_all = "camelCase")]
    pub enum AppMessage {
        StartServer {
            username: String,
            is_ipv6: bool,
            use_upnp: bool,
            port: u16,
            method: ConnectionMethod,
        },
        Connect {
            username: String,
            session_id: Option<String>,
            isipv6: bool,
            ip: Option<IpAddr>,
            hostname: Option<String>,
            port: Option<u16>,
            method: ConnectionMethod,
        },
        TransferControl {
            target: String,
        },
        SetObserver {
            target: String,
            is_observer: bool,
        },
        LoadAircraft {
            config_file_name: String,
            sim: String,
        },
        Disconnect,
        Startup,
        RunUpdater,
        ForceTakeControl,
        UpdateConfig {
            new_config: simconfig::Config,
        },
        GoObserver,
        EmulatorRequestVars,
        EmulatorAddVar {
            name: String,
        },
        EmulatorRemoveVar {
            name: String,
        },
        EmulatorSetVar {
            name: String,
            value: f64,
        },
    }

    pub struct App {
        rx: Receiver<AppMessage>,
        exited: Arc<AtomicBool>,
    }

    impl App {
        pub fn setup(title: String) -> Self {
            let (tx, rx) = unbounded();
            let exited = Arc::new(AtomicBool::new(false));
            let plan = startup_plan();

            println!("[STARTUP] {}", title);
            println!("[STARTUP] Headless console frontend active.");

            match plan.mode {
                StartupMode::Host => {
                    let definition = plan
                        .definition_file
                        .as_ref()
                        .expect("--definition-file is required when hosting");

                    // Must be processed before StartServer, because the normal GUI
                    // ordinarily sends LoadAircraft when an aircraft is selected.
                    tx.try_send(AppMessage::LoadAircraft {
                        config_file_name: definition.clone(),
                        sim: plan.sim.clone(),
                    })
                    .ok();

                    tx.try_send(AppMessage::StartServer {
                        username: plan.name.clone(),
                        is_ipv6: plan.ipv6,
                        use_upnp: plan.use_upnp,
                        port: plan.port,
                        method: plan.connection_method.clone(),
                    })
                    .ok();
                }
                StartupMode::Connect => {
                    let (ip, hostname, port, session_id) = match &plan.connection_method {
                        ConnectionMethod::CloudServer => (
                            None,
                            None,
                            None,
                            plan.session_id
                                .as_ref()
                                .map(|value| value.trim().to_uppercase()),
                        ),
                        ConnectionMethod::Direct => {
                            let target = plan.ip.as_deref().unwrap_or_default().trim();
                            match target.parse::<IpAddr>() {
                                Ok(ip) => (Some(ip), None, Some(plan.port), None),
                                Err(_) => (
                                    None,
                                    Some(target.to_string()),
                                    Some(plan.port),
                                    None,
                                ),
                            }
                        }
                        ConnectionMethod::Relay => {
                            eprintln!(
                                "[ERROR] Relay is a hosting mode; join with cloud-server or direct."
                            );
                            exited.store(true, Ordering::SeqCst);
                            return Self { rx, exited };
                        }
                    };

                    tx.try_send(AppMessage::Connect {
                        username: plan.name.clone(),
                        session_id,
                        isipv6: plan.ipv6,
                        ip,
                        hostname,
                        port,
                        method: plan.connection_method.clone(),
                    })
                    .ok();
                }
            }

            Self { rx, exited }
        }

        pub fn exited(&self) -> bool {
            self.exited.load(Ordering::SeqCst)
        }

        pub fn get_next_message(&self) -> Result<AppMessage, TryRecvError> {
            self.rx.try_recv()
        }

        fn fatal(&self, prefix: &str, msg: &str) {
            eprintln!("[{}] {}", prefix, msg);
            self.exited.store(true, Ordering::SeqCst);
        }

        pub fn error(&self, msg: &str) {
            self.fatal("ERROR", msg);
        }

        pub fn attempt(&self) {
            println!("[STATUS] Attempting connection...");
        }

        pub fn connected(&self) {
            println!("[STATUS] Connected to host.");
        }

        pub fn server_fail(&self, reason: &str) {
            self.fatal("SERVER ERROR", reason);
        }

        pub fn client_fail(&self, reason: &str) {
            self.fatal("CLIENT ERROR", reason);
        }

        pub fn gain_control(&self) {
            println!("[CONTROL] You have control.");
        }

        pub fn lose_control(&self) {
            println!("[CONTROL] Another participant has control.");
        }

        pub fn server_started(&self) {
            println!("[STATUS] Server/session established.");
        }

        pub fn set_session_code(&self, code: &str) {
            println!("[SESSION] {}", code);
        }

        pub fn new_connection(&self, name: &str) {
            println!("[CLIENT] {} joined.", name);
        }

        pub fn lost_connection(&self, name: &str) {
            println!("[CLIENT] {} left.", name);
        }

        pub fn observing(&self, observing: bool) {
            println!(
                "[CONTROL] Observer mode {}.",
                if observing { "enabled" } else { "disabled" }
            );
        }

        pub fn set_observing(&self, name: &str, observing: bool) {
            println!("[CONTROL] {} observer={}.", name, observing);
        }

        pub fn set_incontrol(&self, name: &str) {
            println!("[CONTROL] {} has control.", name);
        }

        pub fn add_fs2020_aircraft(&self, _name: &str) {}
        pub fn add_fs2024_aircraft(&self, _name: &str) {}

        pub fn set_aircraft(&self, config: &str) {
            println!("[AIRCRAFT] {}", config);
        }

        pub fn version(&self, version: &str) {
            println!("[UPDATE] New version available: {}", version);
        }

        pub fn update_failed(&self) {
            eprintln!("[UPDATE] Update failed.");
        }

        pub fn send_config(&self, _value: &str) {}

        pub fn send_network(&self, _metrics: &Metrics) {
            // Metrics are intentionally suppressed to avoid terminal spam.
        }

        pub fn set_host(&self) {
            println!("[STATUS] Hosting established.");
        }

        pub fn emulator_enabled(&self, enabled: bool) {
            if enabled {
                println!("[EMULATOR] Enabled.");
            }
        }

        pub fn send_emulator_vars(&self, _value: &str) {}
        pub fn send_emulator_var_value(&self, _value: &str) {}

        pub fn emulator_error(&self, reason: &str) {
            eprintln!("[EMULATOR ERROR] {}", reason);
        }
    }
}

mod cli {
    use std::sync::OnceLock;

    use clap::{ArgGroup, Parser, ValueEnum};

    use crate::app::ConnectionMethod;
    use crate::simconfig::Config;

    #[derive(Copy, Clone, Debug, ValueEnum)]
    #[value(rename_all = "kebab-case")]
    enum CliConnectionMethod {
        Direct,
        Relay,
        CloudServer,
    }

    #[derive(Copy, Clone, Debug, ValueEnum)]
    #[value(rename_all = "kebab-case")]
    enum CliSim {
        Fs2020,
        Fs2024,
    }

    #[derive(Parser, Debug)]
    #[command(
        author,
        version,
        about = "Headless YourControls client for Wine/Proton",
        long_about = None,
        group(
            ArgGroup::new("mode")
                .required(true)
                .multiple(false)
                .args(["start_server", "connect"])
        )
    )]
    struct Cli {
        #[arg(
            long,
            requires = "definition_file",
            help = "Host a YourControls session."
        )]
        start_server: bool,

        #[arg(long, help = "Join an existing YourControls session.")]
        connect: bool,

        #[arg(
            long,
            help = "Aircraft definition filename. Required when hosting."
        )]
        definition_file: Option<String>,

        #[arg(
            long,
            value_enum,
            default_value_t = CliSim::Fs2020,
            help = "Simulator definition tree."
        )]
        sim: CliSim,

        #[arg(
            long,
            value_enum,
            default_value_t = CliConnectionMethod::CloudServer,
            help = "Connection method."
        )]
        connection_method: CliConnectionMethod,

        #[arg(long, help = "Session code when joining through cloud-server.")]
        session_id: Option<String>,

        #[arg(long, help = "IP address or hostname when joining directly.")]
        ip: Option<String>,

        #[arg(long, default_value_t = 25071, help = "Network port.")]
        port: u16,

        #[arg(long, default_value_t = 5, help = "Connection timeout in seconds.")]
        conn_timeout: u64,

        #[arg(long, default_value = "Pilot", help = "YourControls user name.")]
        name: String,

        #[arg(long, help = "Use IPv6.")]
        ipv6: bool,

        #[arg(long, help = "Disable UPnP for direct hosting.")]
        no_upnp: bool,

        #[arg(long, help = "Enable instructor mode.")]
        instructor_mode: bool,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum StartupMode {
        Host,
        Connect,
    }

    #[derive(Clone, Debug)]
    pub struct StartupPlan {
        pub mode: StartupMode,
        pub definition_file: Option<String>,
        pub sim: String,
        pub connection_method: ConnectionMethod,
        pub session_id: Option<String>,
        pub ip: Option<String>,
        pub port: u16,
        pub conn_timeout: u64,
        pub name: String,
        pub ipv6: bool,
        pub use_upnp: bool,
        pub instructor_mode: bool,
    }

    static STARTUP_PLAN: OnceLock<StartupPlan> = OnceLock::new();

    pub fn startup_plan() -> &'static StartupPlan {
        STARTUP_PLAN
            .get()
            .expect("CLI startup plan was not initialized")
    }

    pub struct CliWrapper {
        plan: StartupPlan,
    }

    impl CliWrapper {
        pub fn new() -> Self {
            let cli = Cli::parse();

            let method = match cli.connection_method {
                CliConnectionMethod::Direct => ConnectionMethod::Direct,
                CliConnectionMethod::Relay => ConnectionMethod::Relay,
                CliConnectionMethod::CloudServer => ConnectionMethod::CloudServer,
            };

            if cli.connect {
                match &method {
                    ConnectionMethod::CloudServer => {
                        let missing = cli
                            .session_id
                            .as_deref()
                            .map(str::trim)
                            .map(str::is_empty)
                            .unwrap_or(true);
                        if missing {
                            eprintln!(
                                "error: --session-id is required with --connect --connection-method cloud-server"
                            );
                            std::process::exit(2);
                        }
                    }
                    ConnectionMethod::Direct => {
                        let missing = cli
                            .ip
                            .as_deref()
                            .map(str::trim)
                            .map(str::is_empty)
                            .unwrap_or(true);
                        if missing {
                            eprintln!(
                                "error: --ip is required with --connect --connection-method direct"
                            );
                            std::process::exit(2);
                        }
                    }
                    ConnectionMethod::Relay => {
                        eprintln!(
                            "error: relay is a hosting mode; join with cloud-server or direct"
                        );
                        std::process::exit(2);
                    }
                }
            }

            let mode = if cli.start_server {
                StartupMode::Host
            } else {
                StartupMode::Connect
            };

            let plan = StartupPlan {
                mode,
                definition_file: cli.definition_file,
                sim: match cli.sim {
                    CliSim::Fs2020 => "FS2020".to_string(),
                    CliSim::Fs2024 => "FS2024".to_string(),
                },
                connection_method: method,
                session_id: cli.session_id,
                ip: cli.ip,
                port: cli.port,
                conn_timeout: cli.conn_timeout,
                name: cli.name,
                ipv6: cli.ipv6,
                use_upnp: !cli.no_upnp,
                instructor_mode: cli.instructor_mode,
            };

            STARTUP_PLAN
                .set(plan.clone())
                .expect("CLI startup plan initialized twice");

            Self { plan }
        }

        pub fn skip_sim_connect(&self) -> bool {
            false
        }

        // The console App injects LoadAircraft/StartServer/Connect itself,
        // so disable the browser-oriented auto-start path.
        pub fn definition_file(&self) -> Option<&str> {
            None
        }

        pub fn start_server(&self) -> bool {
            false
        }

        pub fn emulator_enabled(&self) -> bool {
            false
        }

        pub fn connection_method(&self) -> ConnectionMethod {
            self.plan.connection_method.clone()
        }

        pub fn apply_config_overrides(&self, config: &mut Config) {
            config.conn_timeout = self.plan.conn_timeout;
            config.port = self.plan.port;
            config.name = self.plan.name.clone();
            config.instructor_mode = self.plan.instructor_mode;

            if let Some(ip) = self.plan.ip.as_ref() {
                config.ip = ip.clone();
            }
        }
    }
}

use cli::CliWrapper;
use program::Program;
use simplelog::{
    CombinedLogger, Config as LogConfig, LevelFilter, SharedLogger, SimpleLogger, WriteLogger,
};
use std::{env, fs::File};

const LOG_FILENAME: &str = "log-cli.txt";

fn main() {
    let cli = CliWrapper::new();

    if !cfg!(debug_assertions) {
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                env::set_current_dir(parent).ok();
            }
        }
    }

    let loggers: Vec<Box<dyn SharedLogger>> = vec![
        SimpleLogger::new(LevelFilter::Info, LogConfig::default()),
        WriteLogger::new(
            LevelFilter::Info,
            LogConfig::default(),
            File::create(LOG_FILENAME).expect("Could not create log-cli.txt"),
        ),
    ];
    CombinedLogger::init(loggers).ok();

    let mut program = Program::new(cli);
    program.run();
}
