//! libp2p request_response codec for Foretias JSON-RPC over multiplexed streams.
//!
//! Implements `libp2p::request_response::Codec` so the swarm can send/receive
//! JSON-RPC 2.0 requests over existing yamux/mplex streams.
//!
//! Framing: 4-byte big-endian length prefix + raw JSON-RPC 2.0 bytes.

use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::request_response::Codec;
use libp2p::StreamProtocol;
use std::io;

#[derive(Clone, Debug)]
pub struct ForetiasRpcCodec;

#[async_trait]
impl Codec for ForetiasRpcCodec {
    type Protocol = StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_length_prefixed(io).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_length_prefixed(io).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &req).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &res).await
    }
}

async fn read_length_prefixed<T>(io: &mut T) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin,
{
    let len = {
        let mut buf = [0u8; 4];
        io.read_exact(&mut buf).await?;
        u32::from_be_bytes(buf) as usize
    };
    const MAX_RPC_FRAME_BYTES: usize = 16 * 1024 * 1024;
    if len > MAX_RPC_FRAME_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "RPC frame too large"));
    }
    let mut payload = vec![0u8; len];
    io.read_exact(&mut payload).await?;
    Ok(payload)
}

async fn write_length_prefixed<T>(io: &mut T, data: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin,
{
    let len = (data.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(data).await?;
    Ok(())
}
