use std::net::{SocketAddr, IpAddr, Ipv4Addr, SocketAddrV4};

use anyhow::{Result, bail};
// use tracing::debug;


pub fn bind_multicast(
    multi_addr: &str,
    if_addr: Option<&str>,
) -> Result<std::net::UdpSocket> {
    let multi_addr: SocketAddr = multi_addr.parse()?;

    let if_addr = match if_addr {
        Some(v) => Some(v.parse()?),
        None => None,
    };

    bind_multicast_ip(&multi_addr, if_addr)
}

pub fn bind_multicast_ip(
    multi_addr: &SocketAddr,
    if_addr: Option<IpAddr>,
) -> Result<std::net::UdpSocket> {
    use socket2::{Domain, Type, Protocol, Socket};

    // assert!(multi_addr.ip().is_multicast(), "Must be multcast address");

    match *multi_addr {
        SocketAddr::V4(multi_addr) => { 
            
            let domain = Domain::IPV4;

            let interface = match if_addr {
                Some(v) => match v {
                    IpAddr::V4(v) => v,
                    IpAddr::V6(_) => bail!("multi addr v4 but if addr v6"),
                },
                None => Ipv4Addr::new(0, 0, 0, 0),
            };

            // parse_interface_or(xfer, ||Ok(Ipv4Addr::new(0, 0, 0, 0)))
            // .with_context(||format!("invalid ipv4 [{:?}]", xfer))?;
            
            // debug!("udp addr: multicast [{}], ipv4 iface [{}]", multi_addr, interface);

            // let if_addr = SocketAddr::new(interface.into(), multi_addr.port());

            let socket = Socket::new(
                domain,
                Type::DGRAM,
                Some(Protocol::UDP),
            )?;
            socket.set_reuse_address(true)?;
            socket.set_reuse_port(true)?;
            // // socket.bind(&socket2::SockAddr::from(if_addr))?;
            // // socket.bind(&socket2::SockAddr::from(multi_addr))?;
            // try_bind_multicast(&socket, &multi_addr.into())?;

            let bind_addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), multi_addr.port());
            // let bind_addr = SocketAddrV4::new(interface, multi_addr.port());
            socket.bind(&socket2::SockAddr::from(bind_addr))?;
            

            // 接收端好像没什么用
            // 发送端设置为true时，可以用同一个 socket 收到自己发送的数据
            socket.set_multicast_loop_v4(false)?;  

            // join to the multicast address, with all interfaces
            socket.join_multicast_v4(
                multi_addr.ip(),
                &interface,
            )?;

            Ok(socket.into())
        },
        SocketAddr::V6(multi_addr) => {
            
            let domain = Domain::IPV6;

            // let interface = match if_addr {
            //     Some(v) => match v {
            //         IpAddr::V4(_v) => bail!("multi addr v6 but if addr v4"),
            //         IpAddr::V6(v) => v,
            //     },
            //     None => Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0),
            // };

            // let interface = parse_interface_or(xfer, ||Ok(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)))
            // .with_context(||format!("invalid ipv6 [{:?}]", xfer))?;
            // debug!("udp addr: multicast [{}], ipv6 iface [{}]", multi_addr, interface);

            // let if_addr = SocketAddr::new(interface.into(), multi_addr.port());

            let socket = Socket::new(
                domain,
                Type::DGRAM,
                Some(Protocol::UDP),
            )?;
            // reuse address 是允许多个进程监听同一个地址:端口，但是同一个进程绑定两次会有问题？
            // reuse port 是多个socket负载均衡
            // 参考： https://stackoverflow.com/questions/14388706/how-do-so-reuseaddr-and-so-reuseport-differ
            socket.set_reuse_address(true)?;
            // socket.bind(&socket2::SockAddr::from(if_addr))?;
            // socket.bind(&socket2::SockAddr::from(multi_addr))?;
            try_bind_multicast(&socket, &multi_addr.into())?;

            socket.set_multicast_loop_v6(false)?;

            // join to the multicast address, with all interfaces (ipv6 uses indexes not addresses)
            socket.join_multicast_v6(
                multi_addr.ip(),
                0,
            )?;

            Ok(socket.into())
        },
    }

    
}

/// On Windows, unlike all Unix variants, it is improper to bind to the multicast address
///
/// see https://msdn.microsoft.com/en-us/library/windows/desktop/ms737550(v=vs.85).aspx
#[cfg(windows)]
fn try_bind_multicast(socket: &socket2::Socket, addr: &SocketAddr) -> std::io::Result<()> {
    let addr = match *addr {
        SocketAddr::V4(addr) => SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), addr.port()),
        SocketAddr::V6(addr) => {
            SocketAddr::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0).into(), addr.port())
        }
    };
    socket.bind(&socket2::SockAddr::from(addr))
}

/// On unixes we bind to the multicast address, which causes multicast packets to be filtered
#[cfg(unix)]
fn try_bind_multicast(socket: &socket2::Socket, addr: &SocketAddr) -> std::io::Result<()> {
    socket.bind(&socket2::SockAddr::from(*addr))
}
