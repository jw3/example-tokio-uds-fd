use std::io::IoSlice;
use nix::sys::socket::ControlMessage;

trait ToOutgoing {
    fn to_outgoing(&self) -> OutgoingMsg;
}

trait ToIncoming {
}

struct IncomingMsg<'a> {
    bytes: Vec<u8>,
    cmsg: Vec<ControlMessage<'a>>,
}

struct OutgoingMsg<'a> {
    iov: Vec<IoSlice<'a>>,
    cmsg: Vec<ControlMessage<'a>>,
}

impl<'a> OutgoingMsg<'a> {
    pub fn new<T>(thing: T) -> Self {
        Self {
            iov: vec![],
            cmsg: vec![],
        }
    }
}

fn send(o: &OutgoingMsg) {
    println!("send");
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd;
    use nix::sys::socket::{recvmsg, sendmmsg, sendmsg, MsgFlags, UnixAddr};
    use tokio::net::UnixStream;
    use super::*;

    struct Thing {
        name: String,
        name_bytes: Vec<u8>,
        number: i32,
        number_bytes: Vec<u8>,
    }

    impl Thing {
        fn new(name: &str, number: i32) -> Self {
            Self {
                name: name.to_string(),
                name_bytes: name.as_bytes().to_vec(),
                number,
                number_bytes: number.to_ne_bytes().to_vec(),
            }
        }
    }

    impl ToOutgoing for Thing {
        fn to_outgoing(&self) -> OutgoingMsg {
            let mut outgoing = OutgoingMsg::new(self);
            outgoing.iov.push(IoSlice::new(&self.name_bytes));
            outgoing.iov.push(IoSlice::new(&self.number_bytes));
            outgoing
        }
    }

    // impl<'a> From<&'a Thing> for OutgoingMsg<'a> {
    //     fn from(value: &'a Thing) -> OutgoingMsg<'a> {
    //         let mut outgoing = OutgoingMsg::new(value);
    //         outgoing.iov.push(IoSlice::new(&value.name_bytes));
    //         outgoing.iov.push(IoSlice::new(&value.number_bytes));
    //         outgoing
    //     }
    // }


    #[tokio::test]
    async fn test_outgoing() -> anyhow::Result<()> {
        let t = Thing::new("name", 1);
        let o: OutgoingMsg = t.to_outgoing();

        let sock = UnixStream::connect("/foo/bar.sock").await?;
        unsafe { sendmsg(sock.as_raw_fd(), &o.iov, &o.cmsg, MsgFlags::empty(), None::<&UnixAddr>)?; }

        Ok(())
    }

    // #[tokio::test]
    // async fn test_incoming() -> anyhow::Result<()> {
    //     let sock = UnixStream::connect("/foo/bar.sock").await?;
    //     match recvmsg::<()>(sock.as_raw_fd(), &mut iov, Some(&mut cmsg_buf), MsgFlags::empty()) {
    //         Ok(res) => {}
    //         _ => panic!(),
    //     }
    // }
}
