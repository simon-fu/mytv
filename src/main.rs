/* 
    小米电视启动app的方法
    - curl -v http://192.168.1.10:6095/controller?action=startapp&type=packagename&packagename=org.xbmc.kodi
    - ./adb shell am start org.xbmc.kodi/.Main

    ADB程序下载地址：
    - Windows https://dl.google.com/android/repository/platform-tools-latest-windows.zip
    - MacOS https://dl.google.com/android/repository/platform-tools-latest-darwin.zip
    - Linux https://dl.google.com/android/repository/platform-tools-latest-linux.zip

    参考：
    - https://www.cnblogs.com/zooqkl/p/12708690.html
*/

use std::time::Duration;

use anyhow::{Result, Context};
use clap::Parser;
use time::{UtcOffset, macros::format_description};

use tokio::time::Instant;
use tracing::{level_filters::LevelFilter, debug, info, error};
use tracing_subscriber::{EnvFilter, fmt::{time::OffsetTime, MakeWriter}};

// use crate::multicast::bind_multicast;

// mod multicast;

#[tokio::main]
async fn main() -> Result<()> {
    init_log();

    // let expect_ip: IpAddr = "192.168.1.10".parse().with_context(||"invalid expect ip")?;
    // let timeout = Duration::from_secs(10);
    // let cmd_line = r#"curl -v http://192.168.1.10:6095/controller?action=startapp&type=packagename&packagename=org.xbmc.kodi"#;


    // let tv_ip = "192.168.1.10";
    // let package_name = "org.xbmc.kodi";
    // let timeout: u64 = "3000".parse()?;

    let args = CmdArgs::parse();
    let tv_ip = args.tv_ip.as_str();
    let package_name = args.package_name.as_str();
    let timeout = args.timeout;

    let tv_addr = format!("{tv_ip}:6095");
    let cmd_line = format!("curl -v http://{tv_addr}/controller?action=startapp&type=packagename&packagename={package_name}");
    let cmd_args: Vec<&str> = cmd_line.split(" ").collect();

    let mut last_alive = tcp_connect(tv_addr.as_str(), Duration::from_millis(timeout)).await.is_ok();
    info!("first state [{last_alive}]");

    loop {
        // probe_tv_switch_on_mcast(expect_ip, timeout).await?;
        probe_tv_switch_on_tcp(tv_addr.as_str(), Duration::from_millis(timeout), &mut last_alive).await?;
        exec_command(&cmd_args).await?;
    }
}

async fn exec_command(cmd_args: &[&str]) -> Result<()> {
    let mut command = tokio::process::Command::new(cmd_args[0]);
    for n in 1..cmd_args.len() {
        command.arg(cmd_args[n]);
    }

    let output = command.output().await
    .with_context(||"exec command error")?;

    if !output.status.success() {
        let code = output.status.code();
        let stdout = std::str::from_utf8(&output.stdout);
        let stderr = std::str::from_utf8(&output.stderr);
        error!("exec commnad failed, code [{code:?}], stdout [{stdout:?}], stderr [{stderr:?}]");
    }

    info!("exec cmd ok");

    Ok(())
}

async fn probe_tv_switch_on_tcp(addr: &str, timeout: Duration, last_alive: &mut bool) -> Result<()> {

    loop {
        let kick_time = Instant::now();

        let alive = tcp_connect(addr, timeout).await.is_ok();
        debug!("next state [{last_alive}] -> [{alive}]");

        if alive != *last_alive {
            info!("state changed [{last_alive}] -> [{alive}]");
            let is_on = !*last_alive;
            *last_alive = alive;

            if is_on {
                return Ok(())
            }
        }
        

        let elapsed = kick_time.elapsed();
        if elapsed < timeout {
            tokio::time::sleep(timeout - elapsed).await;
        }
    }
}

async fn tcp_connect(addr: &str, timeout: Duration) -> Result<()> {
    let _r = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr)).await??;
    Ok(())
}


pub(crate) fn init_log() {
    init_log2(||std::io::stdout())
}

pub(crate) fn init_log2<W2>(w: W2) 
where
    W2: for<'writer> MakeWriter<'writer> + 'static + Send + Sync,
{

    // https://time-rs.github.io/book/api/format-description.html
    let fmts = format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");

    let offset = UtcOffset::current_local_offset().expect("should get local offset!");
    let timer = OffsetTime::new(offset, fmts);
    
    let filter = if cfg!(debug_assertions) {
        if let Ok(v) = std::env::var(EnvFilter::DEFAULT_ENV) {
            v.into()
        } else {
            "mytv=debug".into()
            // "debug".into()
        }
    } else {
        EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env_lossy()
    };
        
    tracing_subscriber::fmt()
    .with_max_level(tracing::metadata::LevelFilter::DEBUG)
    .with_env_filter(filter)
    // .with_env_filter("rtun=debug,rserver=debug")
    .with_writer(w)
    .with_timer(timer)
    .with_target(false)
    .init();
}

#[derive(Parser, Debug)]
#[clap(name = "mytv", about, version)]
pub struct CmdArgs {
    #[clap(long="ip", long_help = "tv ip address, for example 192.168.1.10")]
    tv_ip: String,

    #[clap(long="package", long_help = "app package name, for example org.xbmc.kodi")]
    package_name: String,

    #[clap(long="timeout", long_help = "timeout in milliseconds", default_value="3000")]
    timeout: u64,
}
