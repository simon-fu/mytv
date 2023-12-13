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

use std::{time::Duration, net::{IpAddr, SocketAddr}};

use anyhow::{Result, Context};
use clap::Parser;
use time::{UtcOffset, macros::format_description};

use tokio::time::Instant;
use tracing::{level_filters::LevelFilter, debug, info, error};
use tracing_subscriber::{EnvFilter, fmt::{time::OffsetTime, MakeWriter}};

use crate::multicast::bind_multicast;

mod multicast;

#[tokio::main]
async fn main() -> Result<()> {
    init_log();

    // let expect_ip: IpAddr = "192.168.1.10".parse().with_context(||"invalid expect ip")?;
    // let timeout = Duration::from_secs(10);
    // let cmd_line = r#"curl -v http://192.168.1.10:6095/controller?action=startapp&type=packagename&packagename=org.xbmc.kodi"#;

    let args = CmdArgs::parse();
    let tv_ip: IpAddr = args.tv_ip.parse().with_context(||"invalid tv ip")?;
    let package_name = args.package_name.as_str();
    let timeout = Duration::from_millis(args.timeout);

    
    if let Some(multicast_addr) = args.multicast_addr.as_ref() {
        let _r = MulticastSocket::try_new(multicast_addr, None).await
        .with_context(||"bind multicast socket failed")?;
    }

    let tv_addr = format!("{tv_ip}:6095");
    let cmd_line = format!("curl -v http://{tv_addr}/controller?action=startapp&type=packagename&packagename={package_name}");
    let cmd_args: Vec<&str> = cmd_line.split(" ").collect();

    let tcp_prober = TcpProber::new(tv_addr, timeout);

    let last_alive = tcp_prober.probe().await;
    info!("tv init state [{last_alive}]");

    loop {
        tcp_prober.probe_until_off().await?;
        info!("tv switch off");

        if let Some(multicast_addr) = args.multicast_addr.as_ref() {

            let socket = MulticastSocket::try_new(multicast_addr, None).await
            .with_context(||"bind multicast socket failed")?;
            let mut buf = vec![0;  1700];

            while !tcp_prober.probe().await {
                let (len, from) = socket.recv_peer(tv_ip, &mut buf).await?;
                debug!("recv tv multicast from [{from}], bytes {len}");
            }
            info!("mulitcast: tv switch on");
        } else {
            debug!("tcp: try probing until tv on");
            tcp_prober.probe_until_on().await?;
            info!("tcp: tv switch on");
        }

        
        debug!("executing cmd [{cmd_line}]");
        exec_command(&cmd_args).await?;
    }
}

async fn exec_command(cmd_args: &[&str]) -> Result<bool> {
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
        Ok(false)
    } else {
        info!("exec cmd ok");
        Ok(true)
    }
}



#[derive(Debug)]
pub struct TcpProber {
    addr: String,
    timeout: Duration,
}

impl TcpProber {
    pub fn new(addr: String, timeout: Duration) -> Self {
        Self {
            addr,
            timeout,
        }
    }

    pub async fn probe_until_on(&self) -> Result<()> {
        self.probe_until(true).await
    }

    pub async fn probe_until_off(&self) -> Result<()> {
        self.probe_until(false).await
    }

    pub async fn probe(&self) -> bool {
        let is_on = self.try_connect().await.is_ok();
        debug!("probe state [{is_on}]");
        is_on
    }

    async fn probe_until(&self, expect: bool) -> Result<()> {
        loop {
            let kick_time = Instant::now();
    
            let alive = self.probe().await;
    
            if alive == expect {
                return Ok(())
            }
            
            let elapsed = kick_time.elapsed();
            if elapsed < self.timeout {
                tokio::time::sleep(self.timeout - elapsed).await;
            }
        }
    }

    async fn try_connect(&self) -> Result<()> {
        let _r = tokio::time::timeout(self.timeout, tokio::net::TcpStream::connect(&self.addr)).await??;
        Ok(())
    }

}

#[derive(Debug)]
pub struct MulticastSocket {
    socket: tokio::net::UdpSocket,
    // buf: Vec<u8>,
}

impl MulticastSocket {
    pub async fn try_new(
        multi_addr: &str,
        if_addr: Option<&str>,
    ) -> Result<Self> {
        let std_socket = bind_multicast(multi_addr, if_addr)?;
        std_socket.set_nonblocking(true)?;
        let socket = tokio::net::UdpSocket::from_std(std_socket)?;

        Ok(Self {
            socket,
            // buf: vec![0; 1700],
        })
    }

    pub async fn recv_peer<'a>(&self, expect_ip: IpAddr, buf: &'a mut [u8]) -> Result<(usize, SocketAddr)> {
        loop {
            let (len, from) = self.socket.recv_from(buf).await?;
            if from.ip() == expect_ip {
                return Ok((len, from) )
            }
        }
    }
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

    #[clap(long="mcast", long_help = "optional multicast address for saving electricity, for example 224.0.0.251:5353")]
    multicast_addr: Option<String>,

    #[clap(long="timeout", long_help = "timeout in milliseconds", default_value="3000")]
    timeout: u64,
}
