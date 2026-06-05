use std::net::UdpSocket;

use super::FrameSink;

/// Streams raw H.264 Annex-B frames over UDP.
/// Receive with: ffplay -f h264 -i udp://127.0.0.1:5555
pub struct UdpSink {
    socket: UdpSocket,
    dest: std::net::SocketAddr,
}

impl UdpSink {
    pub fn bind(dest: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket, dest: dest.parse().expect("invalid udp dest addr") })
    }
}

impl FrameSink for UdpSink {
    fn send(&mut self, nal_data: &[u8]) {
        // Raw Annex-B bytes, no framing prefix. ffplay -f h264 expects this format.
        // UDP datagrams > ~64 KB will be fragmented at IP layer — fine for M1.
        // M2 adds proper RTP packetization and MTU-aware splitting.
        let _ = self.socket.send_to(nal_data, self.dest);
    }
}
