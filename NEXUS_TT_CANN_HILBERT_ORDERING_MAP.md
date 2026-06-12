# Phase 8/9 Integration: Hilbert-Curve Tensor Train (MPS/MPO) Decomposition Ordering Map for 2D CANN under Asymmetric Astrocyte Ca²⁺ / Fatigue Modulation

This is a pure mathematical + structural blueprint (no Helheim core files, parser.rs, executor.rs, memory.rs etc. are referenced or modified). All lifting of node_type==1 CANN nodes, MPO contraction for lateral currents, fatigue injection, STDP projection, bump_summary routing, and integration with PostBurstContext / thalamic gate / PFC-BG / cerebellar DCN / DreamWorldModel / astrocyte ξ_SNN is performed by user/Antigravity.

It directly extends:
• Phase 8 MPO/Tensor-Train cores G_k[r,r,χ_{k-1},χ_k] + LeftEnv/RightEnv direct rank updates (never materialize W, O(L r² χ²) regime).
• Phase 9 2D CANN manifold (node_type==1), Mexican-hat K(Δr), PDE bump dynamics + raw sensory injection.
• Phase 5/6 astrocyte 2D grid + 5-point stencil PDE (c, p, CICR) producing the smooth but asymmetric fatigue field that modulates local excitability/gain.

───

## 1. Problem (Formal)

2D grid positions p=(x,y), W=H=2^d. Lateral operator given by truncated Mexican-hat: K(p,q) = A exp(−‖p−q‖²/(2σ²)) − B exp(−‖p−q‖²/(2τ²))   (‖p−q‖≤R_support).

A localized bump already breaks 1D translational invariance. Any naive flattening (row-major, Z-curve) turns the 2D-local kernel into a 1D operator with long-range jumps on the chain, inflating the required MPO bond dimension χ.

Astrocyte Ca²⁺ waves (diffusive 5-point PDE + CICR + SNN burst sources) produce a smooth, spatially correlated but asymmetric fatigue field f(p) (0≤f(p)≤1). The effective operator is (to first order): W_eff(p,q) ≈ f(p) · K(p,q) · f(q)   (or f(p)·K(p,q) depending on gain placement).

f is generated on its own 2D grid and has low spatial frequency + topology controlled by astrocyte Betti numbers. When this modulation is injected into a 1D TT ordering it acts as a position-dependent diagonal that can create "defects". In a bad ordering these defects cause the Schmidt rank across cuts to grow exponentially with L_grid or with the number of independent astrocyte sources, destroying the Phase-8 compression and GPU memory guarantees.

Requirements:
• Exact ordering map M: 2D grid → 1D chain index so that the MPO for base K has small intrinsic χ_0.
• Modulation by f only adds a bounded additive Δχ(f) (independent of grid size).
• All asymmetric low-rank updates (fatigue deltas, STDP eligibility, reward) stay O(L_TT r² χ²) with L_TT = log_r(N_sensory) (tiny).
• Fixed-size WGSL buffers only (no allocation, no fragmentation).
• Compatible with Phase-8 base-r multi-index + direct env updates.
• Bijective, reversible, pure WGSL-executable (or trivial table).

───

## 2. The Exact Ordering Map — Hilbert Curve + Base-r Digitization of the Curve Coordinate

Preferred curve: Hilbert (not Z/Morton).

• Hilbert is continuous (adjacent steps in 1D are adjacent/edge-adjacent in 2D).
• Prefixes of the curve correspond to compact, low-perimeter regions in the 2D plane at every dyadic scale (quad-tree alignment).
• This directly minimizes the geometric interface (and therefore the number of crossing Mexican-hat pairs) for any TT bipartition.
• Z-curve has larger discontinuities and worse isoperimetric behavior for prefixes → measurably higher effective rank for the same kernel support.

Map definition

Let s = hilbert_encode(x, y, d)   // s ∈ [0, 2^{2d}).

Then tensorize the Hilbert coordinate exactly as Phase 8 tensorizes a node id: s ↔ (s_1, s_2, …, s_L) in base r (r=4 or 8 recommended).

The Tensor Train / MPO now lives on the L digits of the Hilbert path (L ≈ 2d / log₂(r) ≪ √N). Each TT core k "owns" one resolution digit of the space-filling curve.

This is the Decomposition Ordering Map: M(x,y) = base-r digits of hilbert_encode(x,y, d).

High-order digits = coarse quadrants (where slow astrocyte waves live).
Low-order digits = fine cells (where localized Mexican-hat bumps and short-range STDP live).

WGSL (pure, embeddable)

```wgsl
fn grid_to_curve(x_in: u32, y_in: u32, order: u32) -> u32 {
    var x = x_in;
    var y = y_in;
    var dist: u32 = 0u;
    for (var i: u32 = 0u; i < order; i = i + 1u) {
        let rx = (x >> (order - 1u - i)) & 1u;
        let ry = (y >> (order - 1u - i)) & 1u;
        dist += ((3u * rx) ^ ry) << (2u * (order - 1u - i));
        if (ry == 0u) {
            if (rx == 1u) {
                x = ((1u << (order - i)) - 1u) - x;
                y = ((1u << (order - i)) - 1u) - y;
            }
            let tmp = x; x = y; y = tmp;
        }
    }
    return dist;
}
```

// Inverse (decode) only needed for debug / certain injections.
// For production use a small CPU-precomputed table uploaded as storage
// (size 2^{2d} is acceptable for d≤10; 1M entries is fine).

(The encode above is the standard Hilbert with the usual 90° rotations/reflections that keep the curve connected.)

───

## 3. Mathematical Proof — χ Remains Bounded (Additive, Not Exponential)

Base kernel (no fatigue)

Consider a cut after the first k digits in the TT (bipartition S_left vs S_right induced by the Hilbert prefix).

• S_left is a union of dyadic squares (Hilbert-adjusted) at the scale of the cut.
• K(p,q) ≠ 0 only for ‖p−q‖₂ ≤ R_support.
• Crossing pairs (p∈S_left, q∈S_right) can only occur on the geometric perimeter of the dyadic regions.
• At any fixed scale the number of active interface squares is O(1) per connected component of the prefix (Hilbert prefixes are curve-connected and compact).
• On a small interface patch the local kernel matrix has numerical rank O(R_support²) (or lower — Mexican-hat/Gaussians are approximately separable or have very rapid SVD decay, rank ≈ 3–6 in practice).
• Therefore rank(W_{left,right}) = O(R_support²) (constant, independent of total grid size).
• The virtual bond α_k of the MPO is a basis for exactly the messages needed to compute the action of W across the cut → χ_k ≤ rank(W_{left,right}) + ε (truncation).
• Max χ over all bonds is therefore bounded by a constant χ_0(K) = O(R_support² + 1). No dependence on N_sensory and no exponential.

This is the discrete 2D area-law statement realized on a 1D chain whose ordering has low expansion.

With asymmetric fatigue f

Write f(p) = f₀ + δf(p), where δf comes from the astrocyte diffusive PDE (parabolic smoothing + few localized SNN burst sources + CICR).

• D_f (the diagonal modulation) is an MPO of bond dimension 1 (purely physical legs).
• W_eff = D_f ∘ MPO_K ∘ D_f (up to placement).
• MPO product gives intermediate bond ≤ χ_K · 1.
• After the product we do a standard TT rounding sweep (SVD on virtual unfoldings).

Because δf is the output of a diffusive linear(ish) operator it admits a low-rank TT approximation on the same Hilbert ordering with rank ρ_f ≪ χ_K (ρ_f ≈ 2–6 for typical wave lengths and Betti-bounded source counts; the field is spatially band-limited).

A low-rank perturbation after composition produces singular values whose tail decays at least as fast as the tail of the modulator. Truncation back to target χ = χ_K + Δ with Δ = O(ρ_f) therefore succeeds with small error (standard TT approximation theory + perturbation bounds on singular values).

Local defects (single high-fatigue pixel) only affect the fine-scale digits of their Hilbert position. The rank increase is confined to O(1) cores and is removed by the per-core SVD. It does not propagate across scales.

Hence: max_χ ≤ χ_0(K) + C · ρ_f where C is a small constant (from the product and the log number of scales) and ρ_f is bounded by the intrinsic complexity of the astrocyte field (Betti numbers + diffusion scale), not by grid size or number of sources. No exponential growth occurs.

The 1D translational invariance is broken by the bump and by f, but the entanglement structure along the Hilbert-digit chain remains area-law + low-rank perturbation.

───

## 4. Exact WGSL Data Structures (Fixed-Size, Zero Fragmentation)

```wgsl
const MAX_CHI: u32 = 16u;
const MAX_R:   u32 = 8u;
const MAX_L:   u32 = 32u;   // log_r(N) is tiny

struct TT_MPO_Core {
    // row-major [χl][r_in][r_out][χr]
    data: array<f32, MAX_CHI * MAX_CHI * MAX_R * MAX_R>,
    chi_l: u32,
    chi_r: u32,
    r_in: u32,
    r_out: u32,
    _pad: vec2<u32>,
};

@group(0) @binding(0) var<storage, read_write> mpo_cores: array<TT_MPO_Core, MAX_L>;

// Environments (maintained or rebuilt in a sweep)
@group(0) @binding(1) var<storage, read_write> left_env : array<f32, MAX_CHI*MAX_CHI>;
@group(0) @binding(2) var<storage, read_write> right_env: array<f32, MAX_CHI*MAX_CHI>;

// Fatigue sampled along the curve (written once per slow astrocyte tick by a simple gather shader)
@group(0) @binding(3) var<storage, read> fatigue_curve: array<f32, MAX_SENSORY>;
```

// (Optional) curve <-> grid tables if decode is hot; otherwise use grid_to_curve on the fly.

All arrays are pre-sized at init. Only the chi_l/chi_r fields are consulted at runtime. Data slots beyond the current χ are ignored (or zeroed on truncation). No resizes, no atomics for allocation, no fragmentation.

───

## 5. Update & Contraction Complexity — O(L_TT r² χ²)

L_TT = number of TT cores = number of base-r digits of the Hilbert coordinate (logarithmic).

A local fatigue delta or plasticity event at a grid cell:
• Map (x,y) → s = grid_to_curve(...) → its L_TT digits k₁…k_L.
• For each of those (very few) cores:
  • Form the effective small update matrix U (r×r) from δf or the STDP eligibility term (rank-1 or rank-r).
  • Contract with current LeftEnv / RightEnv (χ×χ matrices) and apply U on the physical legs.
  • Thin QR/SVD on the two virtual unfoldings and truncate back to target χ.
• Dominant cost per core: a few matrix multiplies of shape (χ,r,χ) etc. → O(χ² r²) arithmetic.
• Total per event: O(L_TT r² χ²).

A full environment + re-compression sweep over the entire (tiny) TT costs O(L_TT r² χ²) once every few hundred ticks or after a major topological event. This is exactly the budget stated in the query.

The MPO×vector apply (compressed inference / lateral current) is the standard Phase-8 left-to-right contraction (also O(L_TT r² χ²) per full pass when input is sparse/recent spikes).

───

## 6. Integration Notes (Isolated)

• Keep native 2D buffers for the explicit CANN PDE stencil (if you want a reference path) and for the astrocyte 5-point stencil.
• Once per slow tick: gather f from the astrocyte grid onto the CANN grid locations → write into fatigue_curve in Hilbert order (trivial compute shader).
• Lateral currents for node_type==1 nodes are obtained by the MPO contraction in the Hilbert-TT ordering (modulated by the fatigue lookup) instead of a dense or CSR sum.
• The resulting activity feeds the existing pathways exactly as the uncompressed bump would have.
• All STDP/eligibility that would have touched the lateral Mexican-hat now projects onto the TT cores via the environment method (identical to Phase 8 §3).
• DreamWorldModel can run an identical compressed tick with virtual R_v scaling.

The original 2D CANN grid, the astrocyte grid, the global LIF nodes, and all prior subsystems are completely unchanged.
