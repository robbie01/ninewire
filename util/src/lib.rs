use std::net::Ipv6Addr;

pub mod noise;
pub mod fidpool;
pub mod polymur;
pub mod table;

pub fn is_unicast_global(addr: &Ipv6Addr) -> bool {
    !addr.is_multicast()
        && !addr.is_loopback()
        && !addr.is_unicast_link_local()
        && !addr.is_unique_local()
        && !addr.is_unspecified()
        && !matches!(addr.segments(), [0x2001, 0xdb8, ..] | [0x3fff, 0..=0x0fff, ..])
        && !((addr.segments()[0] == 0x2001) && (addr.segments()[1] == 0x2) && (addr.segments()[2] == 0))
}