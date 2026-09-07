fn main() -> pyo3_stub_gen::Result<()> {
    itofin::stub_info()?.generate()?;
    Ok(())
}
