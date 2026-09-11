use nanoid::alphabet::SAFE;
use serde::{Deserialize, Serialize};
use std::ffi::c_char;
use std::fmt;
use std::io::{Seek, SeekFrom};
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::{
    env::home_dir,
    error::Error,
    fmt::Display,
    fs::{OpenOptions, create_dir, exists},
    io::{Read, Write},
};
use tiny_http::{Response, StatusCode};

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    bind_server: String,
    api_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_server: String::from("0.0.0.0:8892"),
            api_key: nanoid::nanoid!(16, &SAFE),
        }
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Bind Server : {}", self.bind_server)?;
        writeln!(f, "API Key     : {}", self.api_key)?;
        Ok(())
    }
}

fn generate_config() -> Result<Config, Box<dyn Error>> {
    let home_dir = home_dir().unwrap();
    let dir_path = home_dir.join(".config/unlockd");
    let file_path = home_dir.join(".config/unlockd/unlockd.toml");

    if !exists(&dir_path)? {
        create_dir(&dir_path).unwrap_or_default();
    }

    let mut buf = String::new();
    let mut open_options = OpenOptions::new();

    let mut file = open_options
        .read(true)
        .write(true)
        .create(true)
        .open(&file_path)
        .expect("");

    file.read_to_string(&mut buf)?;

    let config = if let Ok(config) = toml::from_str::<Config>(&buf) {
        config
    } else {
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        let config = Config::default();
        let toml = toml::to_string(&config)?;
        println!(
            "[INFO] Didn't find any config, writing Default config to {:?} : \n{}",
            file_path, config
        );

        file.write_all(toml.as_bytes())?;
        config
    };

    Ok(config)
}

#[derive(Debug, Clone, Copy)]
enum AuthResult {
    Approved,
    TimedOut,
}

const DEBUG_LOG_PATH: &str = "/tmp/pam_auth_baby_debug.log";

fn log_debug(msg: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(DEBUG_LOG_PATH)
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{ts}] {msg}");
    }
}

fn server(config: &Config, sender: Sender<AuthResult>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let server = tiny_http::Server::http(&config.bind_server)?;
    log_debug(&format!("Running Server on {}", server.server_addr()));
    loop {
        let req = server.recv()?;
        let response = Response::new_empty(StatusCode::from(200));
        if req.url() != "/auth_baby" {
            log_debug(&format!("rejected request to {}", req.url()));
            req.respond(response.with_status_code(404))?;
            continue;
        }
        let Some(header) = req.headers().iter().find(|h| h.field.equiv("X-API-Key")) else {
            log_debug("missing X-API-Key header");
            req.respond(response.with_status_code(401))?;
            continue;
        };
        if header.value == config.api_key {
            log_debug("auth approved");
            req.respond(response.with_status_code(200))?;
            sender.send(AuthResult::Approved)?;
            break;
        }
        req.respond(response.with_status_code(401))?;
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_setcred(
    _pamh: *mut pam::ffi::pam_handle_t,
    _flags: i32,
    _argc: i32,
    _argv: *const *const c_char,
) -> i32 {
    pam::ffi::PAM_SUCCESS
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pam_sm_authenticate(
    _pamh: *mut pam::ffi::pam_handle_t,
    _flags: i32,
    _argc: i32,
    _argv: *const *const c_char,
) -> i32 {
    let config = match generate_config() {
        Ok(c) => c,
        Err(_) => {
            log_debug("failed to generate config");
            return pam::ffi::PAM_AUTH_ERR;
        }
    };
    let (tx, rx) = std::sync::mpsc::channel::<AuthResult>();
    std::thread::spawn(move || match server(&config, tx) {
        Ok(_) => log_debug("server exited"),
        Err(e) => log_debug(&format!("server error: {e}")),
    });

    match rx.recv_timeout(AUTH_TIMEOUT) {
        Ok(AuthResult::Approved) => pam::ffi::PAM_SUCCESS,
        Ok(AuthResult::TimedOut) => pam::ffi::PAM_AUTH_ERR,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            log_debug("authentication timed out");
            pam::ffi::PAM_AUTH_ERR
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log_debug("server thread exited unexpectedly");
            pam::ffi::PAM_AUTH_ERR
        }
    }
}
