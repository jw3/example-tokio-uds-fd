use std::io::IoSlice;
use nix::sys::socket::ControlMessage;

trait ToOutgoing {
    type H;
    type P;
    fn to_outgoing<'a>(&self, h: &'a Self::H, p: &'a Self::P) ->  OutgoingMsg<'a> ;
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

fn send<H, P>(o: &OutgoingMsg) {
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
        number: i32,
    }

    struct ThingH {
        name_len: [u8; 2],
        number_len: [u8; 2],
    }

    struct ThingP {
        name_bytes: Vec<u8>,
        number_bytes: Vec<u8>,
    }

    impl From<&ThingP> for ThingH {
        fn from(value: &ThingP) -> Self {
            Self {
                name_len: value.name_bytes.clone().try_into().unwrap(),
                number_len: value.number_bytes.clone().try_into().unwrap(),
            }
        }
    }

    impl From<&Thing> for ThingP {
        fn from(value: &Thing) -> Self {
            Self {
                name_bytes: value.name.as_bytes().to_vec(),
                number_bytes: value.number.to_be_bytes().to_vec(),
            }
        }
    }

    impl Thing {
        fn new(name: &str, number: i32) -> Self {
            Self {
                name: name.to_string(),
                number,
            }
        }
    }

    impl ToOutgoing for Thing {
        type H = ThingH;
        type P = ThingP;
        fn to_outgoing<'a>(&self, h: &'a Self::H, p: &'a Self::P) -> OutgoingMsg<'a> {
            let mut outgoing = OutgoingMsg::new(self);
            outgoing.iov.push(IoSlice::new(&h.name_len));
            outgoing.iov.push(IoSlice::new(&h.number_len));
            outgoing.iov.push(IoSlice::new(&p.name_bytes));
            outgoing.iov.push(IoSlice::new(&p.number_bytes));
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
        let p: ThingP = (&t).into();
        let h: ThingH = (&p).into();
        let o = t.to_outgoing(&h, &p);

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
