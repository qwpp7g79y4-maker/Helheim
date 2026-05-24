use cudarc::driver::{CudaContext, CudaStream, LaunchConfig};
fn main() {
    let dev = CudaContext::new(0).unwrap();
    let ptx_src = r#"
.version 7.5
.target sm_86
.address_size 64

.visible .entry simple_add(
    .param .u64 A,
    .param .u64 B,
    .param .u64 C,
    .param .u32 N
)
{
    .reg .u64 %A, %B, %C;
    .reg .u32 %N, %idx;
    .reg .f32 %a, %b, %c;
    .reg .pred %p;

    ld.param.u64 %A, [A];
    ld.param.u64 %B, [B];
    ld.param.u64 %C, [C];
    ld.param.u32 %N, [N];

    mov.u32 %idx, %tid.x;
    setp.ge.u32 %p, %idx, %N;
    @%p bra END;

    mul.wide.u32 %idx, %idx, 4; // float is 4 bytes
    add.u64 %A, %A, %idx;
    add.u64 %B, %B, %idx;
    add.u64 %C, %C, %idx;

    ld.global.f32 %a, [%A];
    ld.global.f32 %b, [%B];
    add.f32 %c, %a, %b;
    st.global.f32 [%C], %c;

END:
    ret;
}
    "#;
    let module = dev.load_ptx(cudarc::nvrtc::Ptx::from_src(ptx_src), "simple_add", &["simple_add"]).unwrap();
    println!("Loaded PTX successfully!");
}
