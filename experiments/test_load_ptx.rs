fn main() {
    let dev = cudarc::driver::CudaContext::new(0).unwrap();
    let ptx_src = ".version 7.5\n.target sm_86\n.address_size 64\n.visible .entry my_kernel() { ret; }";
    let ptx = cudarc::nvrtc::Ptx::from_src(ptx_src);
    dev.load_ptx(ptx, "my_module", &["my_kernel"]).unwrap();
    println!("Loaded!");
}
