#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde_json::json;
use tauri::{AppHandle, Runtime};
use uuid::Uuid;
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

use crate::{logging::emit_log, models::AppSettings, runtime_paths};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(windows)]
struct KillOnCloseJob {
    _handle: OwnedHandle,
}

#[cfg(windows)]
impl KillOnCloseJob {
    fn assign(child: &Child) -> Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error()).context("create mihomo job object");
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle() as HANDLE,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error()).context("configure mihomo job object");
        }

        let assigned = unsafe {
            AssignProcessToJobObject(
                handle.as_raw_handle() as HANDLE,
                child.as_raw_handle() as HANDLE,
            )
        };
        if assigned == 0 {
            return Err(std::io::Error::last_os_error()).context("assign mihomo to job object");
        }

        Ok(Self { _handle: handle })
    }
}

struct RunningProcess {
    child: Child,
    #[cfg(windows)]
    _job: KillOnCloseJob,
    controller_port: u16,
    controller_secret: String,
}

#[derive(Default)]
pub struct RunnerHandle {
    child: Mutex<Option<RunningProcess>>,
    operation: Mutex<()>,
}

impl RunnerHandle {
    fn process_guard(&self) -> MutexGuard<'_, Option<RunningProcess>> {
        self.child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn operation_guard(&self) -> MutexGuard<'_, ()> {
        self.operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn allocate_local_port() -> Result<u16> {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").context("bind controller port")?;
        Ok(listener
            .local_addr()
            .context("read controller address")?
            .port())
    }

    fn wait_controller_ready(
        child: &mut Child,
        controller_port: u16,
        controller_secret: &str,
    ) -> Result<()> {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(500))
            .build()
            .context("build controller readiness client")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let url = format!("http://127.0.0.1:{controller_port}/version");
        let mut last_error = String::new();
        while Instant::now() < deadline {
            if let Some(status) = child.try_wait()? {
                return Err(anyhow!(
                    "mihomo 启动失败，进程已退出 (code={})",
                    status.code().unwrap_or(-1)
                ));
            }
            match client.get(&url).bearer_auth(controller_secret).send() {
                Ok(response) if response.status().is_success() => return Ok(()),
                Ok(response) => last_error = format!("HTTP {}", response.status()),
                Err(error) => last_error = error.to_string(),
            }
            thread::sleep(Duration::from_millis(150));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(anyhow!("mihomo 控制器启动超时: {last_error}"))
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.process_guard();
        if let Some(process) = guard.as_mut() {
            match process.child.try_wait() {
                Ok(Some(_)) => {
                    *guard = None;
                    false
                }
                Ok(None) => true,
                // If Windows cannot query the child state, assume it is still
                // alive so a second mihomo process is never started by mistake.
                Err(_) => true,
            }
        } else {
            false
        }
    }

    pub fn controller_access(&self) -> Option<(u16, String)> {
        let mut guard = self.process_guard();
        let process = guard.as_mut()?;
        match process.child.try_wait() {
            Ok(Some(_)) => {
                *guard = None;
                None
            }
            Ok(None) => Some((process.controller_port, process.controller_secret.clone())),
            Err(_) => None,
        }
    }

    pub fn start<R: Runtime>(&self, app: &AppHandle<R>, settings: &AppSettings) -> Result<()> {
        let config = runtime_paths::pool_config_path()?;
        self.start_from(app, settings, &config)
    }

    pub fn start_from<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        settings: &AppSettings,
        config: &std::path::Path,
    ) -> Result<()> {
        let _operation = self.operation_guard();
        if self.is_running() {
            emit_log(app, "info", "mihomo 已在运行中");
            return Ok(());
        }

        let mihomo = runtime_paths::resolve_mihomo_path(&settings.mihomo_path);
        if !mihomo.is_file() {
            return Err(anyhow!("mihomo.exe 不存在: {}", mihomo.display()));
        }

        if !config.exists() {
            return Err(anyhow!("配置文件不存在: {}", config.display()));
        }

        let controller_port = Self::allocate_local_port()?;
        let controller_secret = Uuid::new_v4().to_string();
        let mut command = Command::new(&mihomo);
        command
            .arg("-f")
            .arg(config)
            .arg("-ext-ctl")
            .arg(format!("127.0.0.1:{controller_port}"))
            .arg("-secret")
            .arg(&controller_secret)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn mihomo: {}", mihomo.display()))?;

        #[cfg(windows)]
        let job = KillOnCloseJob::assign(&child).inspect_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
        })?;

        if let Some(stdout) = child.stdout.take() {
            let app_handle = app.clone();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    emit_log(&app_handle, "info", line);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let app_handle = app.clone();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    emit_log(&app_handle, "error", line);
                }
            });
        }

        Self::wait_controller_ready(&mut child, controller_port, &controller_secret)?;

        emit_log(app, "info", format!("mihomo 已启动 (PID: {})", child.id()));
        let mut guard = self.process_guard();
        *guard = Some(RunningProcess {
            child,
            #[cfg(windows)]
            _job: job,
            controller_port,
            controller_secret,
        });
        Ok(())
    }

    #[allow(dead_code)]
    pub fn start_headless(&self, settings: &AppSettings) -> Result<()> {
        let _operation = self.operation_guard();
        if self.is_running() {
            return Ok(());
        }

        let mihomo = runtime_paths::resolve_mihomo_path(&settings.mihomo_path);
        if !mihomo.is_file() {
            return Err(anyhow!("mihomo.exe 不存在: {}", mihomo.display()));
        }

        let config = runtime_paths::pool_config_path()?;
        if !config.exists() {
            return Err(anyhow!("配置文件不存在: {}", config.display()));
        }

        let controller_port = Self::allocate_local_port()?;
        let controller_secret = Uuid::new_v4().to_string();
        let mut command = Command::new(&mihomo);
        command
            .arg("-f")
            .arg(&config)
            .arg("-ext-ctl")
            .arg(format!("127.0.0.1:{controller_port}"))
            .arg("-secret")
            .arg(&controller_secret)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn mihomo: {}", mihomo.display()))?;

        #[cfg(windows)]
        let job = KillOnCloseJob::assign(&child).inspect_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
        })?;

        Self::wait_controller_ready(&mut child, controller_port, &controller_secret)?;

        let mut guard = self.process_guard();
        *guard = Some(RunningProcess {
            child,
            #[cfg(windows)]
            _job: job,
            controller_port,
            controller_secret,
        });
        Ok(())
    }

    pub fn stop<R: Runtime>(&self, app: &AppHandle<R>) -> Result<()> {
        let _operation = self.operation_guard();
        let mut guard = self.process_guard();
        if let Some(process) = guard.as_mut() {
            process.child.kill().context("kill mihomo")?;
            let _ = process.child.wait();
            emit_log(app, "info", "mihomo 已停止");
        }
        *guard = None;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn stop_headless(&self) -> Result<()> {
        let _operation = self.operation_guard();
        let mut guard = self.process_guard();
        if let Some(process) = guard.as_mut() {
            process.child.kill().context("kill mihomo")?;
            let _ = process.child.wait();
        }
        *guard = None;
        Ok(())
    }

    pub fn reload<R: Runtime>(&self, app: &AppHandle<R>) -> Result<()> {
        let config = runtime_paths::pool_config_path()?;
        self.reload_from(app, &config)
    }

    pub fn reload_from<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        config: &std::path::Path,
    ) -> Result<()> {
        let _operation = self.operation_guard();
        let (controller_port, controller_secret) = {
            let mut guard = self.process_guard();
            let Some(process) = guard.as_mut() else {
                return Err(anyhow!("mihomo 未运行，无法热加载配置"));
            };
            if process.child.try_wait()?.is_some() {
                *guard = None;
                return Err(anyhow!("mihomo 进程已退出，无法热加载配置"));
            }
            (process.controller_port, process.controller_secret.clone())
        };

        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .context("build reload client")?;
        let response = client
            .put(format!(
                "http://127.0.0.1:{controller_port}/configs?force=true"
            ))
            .bearer_auth(controller_secret)
            .json(&json!({ "path": config.display().to_string() }))
            .send()
            .context("send reload request")?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().unwrap_or_default();
            let detail = detail.trim();
            return Err(if detail.is_empty() {
                anyhow!("热加载配置失败: HTTP {status}")
            } else {
                anyhow!("热加载配置失败: HTTP {status}: {detail}")
            });
        }

        emit_log(app, "info", "mihomo 已无感应用新配置");
        Ok(())
    }

    pub fn restart<R: Runtime>(&self, app: &AppHandle<R>, settings: &AppSettings) -> Result<()> {
        self.stop(app)?;
        self.start(app, settings)
    }

    pub fn restart_from<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        settings: &AppSettings,
        config: &std::path::Path,
    ) -> Result<()> {
        self.stop(app)?;
        self.start_from(app, settings, config)
    }
}
