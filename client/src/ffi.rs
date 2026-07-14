//! Kernel ABI: framebuffer + Kindle EPDC structs and ioctl numbers.

// Standard Linux framebuffer ioctl numbers (see <linux/fb.h>).
// Typed as c_ulong (not libc::Ioctl) so the crate still type-checks on
// non-Linux dev hosts where libc::Ioctl isn't defined, like macos
pub(super) const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
pub(super) const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;

// These structs mirror the kernel's `fb_var_screeninfo` and `fb_fix_screeninfo`.
// We only read from them, fields we care about are `xres`, `yres` (visible
// resolution) and `line_length` (stride in bytes per row, which may be larger
// than xres due to alignment padding).

#[repr(C)]
#[derive(Default)]
pub(super) struct FbBitfield {
    pub(super) offset: u32,
    pub(super) length: u32,
    pub(super) msb_right: u32,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct FbVarScreeninfo {
    pub(super) xres: u32,
    pub(super) yres: u32,
    pub(super) xres_virtual: u32,
    pub(super) yres_virtual: u32,
    pub(super) xoffset: u32,
    pub(super) yoffset: u32,
    pub(super) bits_per_pixel: u32,
    pub(super) grayscale: u32,
    pub(super) red: FbBitfield,
    pub(super) green: FbBitfield,
    pub(super) blue: FbBitfield,
    pub(super) transp: FbBitfield,
    pub(super) nonstd: u32,
    pub(super) activate: u32,
    pub(super) height: u32,
    pub(super) width: u32,
    pub(super) accel_flags: u32,
    pub(super) pixclock: u32,
    pub(super) left_margin: u32,
    pub(super) right_margin: u32,
    pub(super) upper_margin: u32,
    pub(super) lower_margin: u32,
    pub(super) hsync_len: u32,
    pub(super) vsync_len: u32,
    pub(super) sync: u32,
    pub(super) vmode: u32,
    pub(super) rotate: u32,
    pub(super) colorspace: u32,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
#[derive(Default)]
pub(super) struct FbFixScreeninfo {
    pub(super) id: [u8; 16],
    pub(super) smem_start: libc::c_ulong,
    pub(super) smem_len: u32,
    pub(super) type_: u32,
    pub(super) type_aux: u32,
    pub(super) visual: u32,
    pub(super) xpanstep: u16,
    pub(super) ypanstep: u16,
    pub(super) ywrapstep: u16,
    pub(super) line_length: u32,
    pub(super) mmio_start: libc::c_ulong,
    pub(super) mmio_len: u32,
    pub(super) accel: u32,
    pub(super) capabilities: u16,
    pub(super) reserved: [u16; 2],
}
