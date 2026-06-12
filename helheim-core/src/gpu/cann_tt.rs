use std::fmt::Debug;

/// Represents the grid coordinate on the 2D Continuous Attractor Neural Network (CANN) manifold.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCoord {
    pub x: f32,
    pub y: f32,
}

/// A single Tensor Train (MPS / MPO) core for the Phase 8/9 Hilbert Curve mapping.
/// It contains a row-major flattening of the 4D tensor `[chi_l][r_in][r_out][chi_r]`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TTMPOCore {
    pub data: [f32; TTMPOCore::MAX_CHI * TTMPOCore::MAX_CHI * TTMPOCore::MAX_R * TTMPOCore::MAX_R],
    pub chi_l: u32,
    pub chi_r: u32,
    pub r_in: u32,
    pub r_out: u32,
    pub _pad: [u32; 2],
}

impl TTMPOCore {
    pub const MAX_CHI: usize = 16;
    pub const MAX_R: usize = 8;
    pub const MAX_L: usize = 32; // log_r(N_sensory)

    pub fn new(chi_l: u32, chi_r: u32, r_in: u32, r_out: u32) -> Self {
        Self {
            data: [0.0; Self::MAX_CHI * Self::MAX_CHI * Self::MAX_R * Self::MAX_R],
            chi_l,
            chi_r,
            r_in,
            r_out,
            _pad: [0; 2],
        }
    }
}

/// Converts a 2D (x,y) grid position into a 1D Hilbert Curve distance (order determines resolution).
/// This perfectly matches the WGSL implementation to ensure CPU/GPU parity.
pub fn grid_to_curve(mut x: u32, mut y: u32, order: u32) -> u32 {
    let mut dist = 0;
    for i in 0..order {
        let rx = (x >> (order - 1 - i)) & 1;
        let ry = (y >> (order - 1 - i)) & 1;
        dist += ((3 * rx) ^ ry) << (2 * (order - 1 - i));
        
        if ry == 0 {
            if rx == 1 {
                x = ((1 << (order - i)) - 1) - x;
                y = ((1 << (order - i)) - 1) - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
    }
    dist
}
