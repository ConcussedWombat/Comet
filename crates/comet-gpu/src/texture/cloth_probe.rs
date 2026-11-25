//! Local-only identification of a suspect cloth texture. No title
//! bytes are exported and no title memory is modified. The marker is
//! intentionally synthetic, for locating a binding, not a rendering fix.

use std::sync::OnceLock;

use super::Staging;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selector {
    address: usize,
    dxgi: i32,
    width: u32,
    height: u32,
    swizzle: u32,
}

fn address(s: &str) -> Option<usize> {
    usize::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16).ok()
}

fn selector(s: &str) -> Option<Selector> {
    let mut fields = s.split(':');
    let result = Selector {
        address: address(fields.next()?)?,
        dxgi: fields.next()?.parse().ok()?,
        width: fields.next()?.parse().ok()?,
        height: fields.next()?.parse().ok()?,
        swizzle: fields.next()?.parse().ok()?,
    };
    fields.next().is_none().then_some(result)
}

struct Config {
    trace: bool,
    trace_address: Option<usize>,
    mark: Option<Selector>,
}

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let filter = std::env::var("COMET_R3_CLOTH_TRACE_ADDR").ok();
        let trace_address = filter.as_deref().and_then(address);
        let trace = std::env::var_os("COMET_R3_CLOTH_TRACE").is_some_and(|v| v == "1")
            && (filter.is_none() || trace_address.is_some());
        let mark = std::env::var("COMET_R3_CLOTH_MARK")
            .ok()
            .as_deref()
            .and_then(selector);
        Config { trace, trace_address, mark }
    })
}

/// A valid constant magenta color (BC1/2/3/7), or a constant channel
/// value for BC4/5. The mode-6 BC7 block has both endpoints identical,
/// p-bits set, and zero indices. No BC6H marker (HDR).
fn marker(dxgi: i32) -> Option<&'static [u8]> {
    match dxgi {
        70..=72 => Some(&[0x1f, 0xf8, 0x1f, 0xf8, 0, 0, 0, 0]),
        73..=75 => Some(&[
            255, 255, 255, 255, 255, 255, 255, 255,
            0x1f, 0xf8, 0x1f, 0xf8, 0, 0, 0, 0,
        ]),
        76..=78 => Some(&[
            255, 255, 0, 0, 0, 0, 0, 0,
            0x1f, 0xf8, 0x1f, 0xf8, 0, 0, 0, 0,
        ]),
        79..=81 => Some(&[255, 255, 0, 0, 0, 0, 0, 0]),
        82..=84 => Some(&[
            255, 255, 0, 0, 0, 0, 0, 0,
            128, 128, 0, 0, 0, 0, 0, 0,
        ]),
        97..=99 => Some(&[
            192, 255, 31, 0, 248, 255, 255, 255,
            1, 0, 0, 0, 0, 0, 0, 0,
        ]),
        _ => None,
    }
}

pub(super) fn inspect(
    staging: &Staging,
    mip: u32,
    slice: u32,
    linear: &mut [u8],
    pitch: usize,
    dxgi: i32,
) {
    let cfg = config();
    if !cfg.trace && cfg.mark.is_none() {
        return;
    }
    let layout = &staging.layout;
    let d = &layout.desc;
    if d.dimension != comet_formats::texture::Dimension::Tex2d || d.block != 4 {
        return;
    }
    let id = Selector {
        address: staging.memory as usize,
        dxgi,
        width: d.width,
        height: d.height,
        swizzle: d.swizzle.0,
    };
    let marking = cfg.mark == Some(id) && marker(dxgi).is_some();
    let trace = cfg.trace
        && cfg.trace_address.is_none_or(|a| a == id.address)
        && (cfg.trace_address.is_some() || mip == 0 || mip + 1 == d.mips);
    if !marking && !trace {
        return;
    }
    let Some(m) = layout.mips.get(mip as usize) else { return };
    let n = d.element_bytes as usize;
    let active = m.width as usize * n;
    let replacement = marker(dxgi).filter(|b| marking && b.len() == n);
    let mut hash = blake3::Hasher::new();
    let (mut blocks, mut zeros, mut invalid_bc7, mut repeated) = (0u64, 0u64, 0u64, 0u64);
    let mut first_bad = None;
    let mut previous = [0u8; 16];
    for y in 0..m.height {
        let start = y as usize * pitch;
        let Some(row) = linear.get_mut(start..start + active) else { return };
        // Digest only active block bytes, never host row padding.
        hash.update(row);
        for (x, block) in row.chunks_exact_mut(n).enumerate() {
            if blocks != 0 && block == &previous[..n] {
                repeated += 1;
            }
            previous[..n].copy_from_slice(block);
            blocks += 1;
            if block.iter().all(|&b| b == 0) {
                zeros += 1;
            }
            if (97..=99).contains(&dxgi) && block[0] == 0 {
                invalid_bc7 += 1;
                first_bad.get_or_insert((x as u32, y));
            }
            if let Some(bytes) = replacement {
                block.copy_from_slice(bytes);
            }
        }
    }
    let first_bad_offset = first_bad.and_then(|(x, y)| layout.offset(mip, x, y, slice).ok());
    let span = layout.subresource_read_span(mip, slice);
    #[cfg(feature = "diagnostics")]
    let frame = crate::queue::presented() + 1;
    #[cfg(not(feature = "diagnostics"))]
    let frame = 0u64;
    tracing::info!(
        frame,
        address = format_args!("{:#x}", id.address),
        dxgi, width = id.width, height = id.height, swizzle = id.swizzle,
        mip, slice, mip_blocks_x = m.width, mip_blocks_y = m.height,
        blocks, zeros, invalid_bc7, repeated,
        ?first_bad, ?first_bad_offset, ?span,
        digest = %hash.finalize(),
        marked = replacement.is_some(),
        "R3-CLOTH"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloth_probe_selector_requires_exact_identity() {
        assert_eq!(
            selector("0x10000:98:2048:1024:10"),
            Some(Selector {
                address: 0x10000, dxgi: 98, width: 2048, height: 1024, swizzle: 10,
            }),
        );
        assert_eq!(selector("0x10000"), None);
        assert_eq!(selector("0x10000:98:2048:1024:10:1"), None);
        assert_eq!(selector("not-an-address:98:2048:1024:10"), None);
    }

    #[test]
    fn cloth_probe_bc7_marker_is_constant_mode6() {
        let bits = u128::from_le_bytes(marker(98).unwrap().try_into().unwrap());
        assert_eq!(bits & 127, 64);
        let fields = [127, 127, 0, 0, 127, 127, 127, 127];
        for (i, value) in fields.into_iter().enumerate() {
            assert_eq!((bits >> (7 + 7 * i)) & 127, value);
        }
        assert_eq!((bits >> 63) & 3, 3);
        assert_eq!(bits >> 65, 0);
        for dxgi in (70..=84).chain(97..=99) {
            let expected = if matches!(dxgi, 70..=72 | 79..=81) { 8 } else { 16 };
            assert_eq!(marker(dxgi).unwrap().len(), expected);
        }
        assert!(marker(95).is_none());
    }
}
