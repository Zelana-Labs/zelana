#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::xdp_action,
    macros::xdp,
    programs::XdpContext,
};

// pub struct XdpContext {
//     pub ctx: *mut xdp_md,
// }
// #[repr(C)]
// pub struct xdp_md {
//     pub data: u32,
//     pub data_end: u32,
//     pub data_meta: u32,
//     pub ingress_ifindex: u32,
//     pub rx_queue_index: u32,
//     pub egress_ifindex: u32,
// }

use core::mem;

//IPv4
const ETH_P_IP: u16 = 0x0800;

const IPPROTO_UDP: u8 = 17;

#[repr(C)]
struct EthHdr {
    dst: [u8; 6],
    src: [u8; 6],
    ethertype: u16,
}

#[repr(C)]
struct Ipv4Hdr {
    version_ihl: u8,
    tos: u8,
    tot_len: u16,
    id: u16,
    frag_off: u16,
    ttl: u8,
    protocol: u8,
    check: u16,
    saddr: u32,
    daddr: u32,
}

#[repr(C)]
struct UdpHdr {
    source: u16,
    dest: u16,
    len: u16,
    check: u16,
}

// Helpers: safe-ish packet parsing (bounds-checked)
fn ptr_at<T>(ctx: &XdpContext, offset: usize) -> Option<*const T> {
    let start = ctx.data() as usize;
    let end = ctx.data_end() as usize;
    let size = mem::size_of::<T>();
    if start + offset + size > end {
        return None;
    }
    Some((start + offset) as *const T)
}

#[xdp]
pub fn xdp_drop_dns(ctx: XdpContext) -> u32 {
    match try_xdp_drop_dns(&ctx) {
        Ok(action) => action,
        Err(_) => xdp_action::XDP_PASS,
    }
}

fn try_xdp_drop_dns(ctx: &XdpContext) -> Result<u32, ()> {
    // Check if ether type is IPv4
    let eth = unsafe { &*ptr_at::<EthHdr>(ctx, 0).ok_or(())? };
    let ethertype = u16::from_be(eth.ethertype);
    if ethertype != ETH_P_IP {
        return Ok(xdp_action::XDP_PASS);
    }

    // Check if protocol is UDP
    let ip_off = mem::size_of::<EthHdr>();
    let ip = unsafe { &*ptr_at::<Ipv4Hdr>(ctx, ip_off).ok_or(())? };
    if ip.protocol != IPPROTO_UDP {
        return Ok(xdp_action::XDP_PASS);
    }

    // IPv4 header length (IHL) is lower 4 bits, in 32-bit words
    let ihl_words = (ip.version_ihl & 0x0f) as usize;
    let ihl_bytes = ihl_words * 4;

    let udp_off = ip_off + ihl_bytes;
    let udp = unsafe { &*ptr_at::<UdpHdr>(ctx, udp_off).ok_or(())? };

    let dst_port = u16::from_be(udp.dest);
    if dst_port == 53 {
        return Ok(xdp_action::XDP_DROP);
    }

    Ok(xdp_action::XDP_PASS)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
