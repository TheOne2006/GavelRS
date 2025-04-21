// core/src/rpc/mod.rs
pub mod message;
use message::Message;
use std::os::unix::net::{UnixListener, UnixStream};
use std::fs;
use bincode::{encode_into_std_write, decode_from_std_read};

// 接收端（返回具体 Message 类型）
pub fn receive_message(socket_path: &str) -> Result<Message, Box<dyn std::error::Error>> {
    let _ = fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    let (mut stream, _) = listener.accept()?;
    
    // 直接指定 Message 类型
    let msg: Message = decode_from_std_read(
        &mut stream,
        bincode::config::standard()
    )?;
    
    Ok(msg)
}

// 发送端（接受 Message 引用）
pub fn send_message(socket_path: &str, obj: &Message) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(socket_path)?;
    
    encode_into_std_write(
        obj,
        &mut stream,
        bincode::config::standard()
    )?;
    
    Ok(())
}