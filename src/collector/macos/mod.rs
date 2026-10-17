#[cfg(target_os = "macos")]
#[derive(Default)]
pub struct MacPlatform;
#[cfg(target_os = "macos")]
impl MacPlatform {
    pub fn new() -> Self {
        MacPlatform
    }
}
#[cfg(target_os = "macos")]
impl crate::collector::Platform for MacPlatform {
    fn sample(&mut self) -> crate::collector::PlatformData {
        Default::default()
    }
}
