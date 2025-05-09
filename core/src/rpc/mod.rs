// core/src/rpc/mod.rs
pub mod message;
use anyhow::{Context, Result};
use bincode::config::standard as bincode_config; // Alias for clarity
use bincode::{decode_from_slice, encode_to_vec}; // Removed unused bincode functions
use message::Message;
use std::io::{Read, Write}; // Import Read and Write
use std::os::unix::net::UnixStream; // Use anyhow for error handling

// Removed the old server-style receive_message function

/// Sends a request message and waits for a reply message.
/// Uses a simple length-prefix framing (u32 length).
pub fn request_reply(socket_path: &str, request: &Message) -> Result<Message> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("Failed to connect to socket: {}", socket_path))?;

    // --- Send Request ---
    let encoded_request =
        encode_to_vec(request, bincode_config()).context("Failed to encode request message")?;
    let request_len = encoded_request.len() as u32;

    stream.write_all(&request_len.to_le_bytes()).context("Failed to write request length")?;
    stream.write_all(&encoded_request).context("Failed to write request data")?;
    stream.flush().context("Failed to flush stream for request")?;

    // --- Receive Reply ---
    let mut reply_len_bytes = [0u8; 4];
    stream.read_exact(&mut reply_len_bytes).context("Failed to read reply length")?;
    let reply_len = u32::from_le_bytes(reply_len_bytes);

    // Basic sanity check for length
    if reply_len > 10 * 1024 * 1024 {
        // e.g., limit reply size to 10MB
        return Err(anyhow::anyhow!("Received excessively large reply length: {}", reply_len));
    }

    let mut reply_buf = vec![0u8; reply_len as usize];
    stream.read_exact(&mut reply_buf).context("Failed to read reply data")?;

    let (reply_message, _): (Message, usize) = decode_from_slice(&reply_buf, bincode_config())
        .context("Failed to decode reply message")?;

    Ok(reply_message)
}
