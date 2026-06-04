use std::net::UdpSocket;

use super::FrameSink;

/// Streams H.264 Annex-B frames over UDP with a 4-byte big-endian length prefix.
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
        // Split into MTU-sized chunks with a 4-byte length prefix on the first chunk.
        // For M1 correctness we send in one shot; M2 adds proper RTP packetization.
        let len = (nal_data.len() as u32).to_be_bytes();
        let mut packet = Vec::with_capacity(4 + nal_data.len());
        packet.extend_from_slice(&len);
        packet.extend_from_slice(nal_data);

        // UDP datagrams > ~64 KB will be fragmented at IP layer — fine for M1.
        let _ = self.socket.send_to(&packet, self.dest);
    }
}
