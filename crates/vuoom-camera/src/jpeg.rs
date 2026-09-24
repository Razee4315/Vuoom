//! JPEG through the Windows Imaging Component: built into every Windows, nothing to
//! bundle, and quick at webcam sizes. Pixels are BGRA with opaque alpha on both sides.

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use windows::core::{Error, Result};
    use windows::Win32::Foundation::{E_FAIL, HGLOBAL};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR,
        GUID_WICPixelFormat32bppBGRA, IWICImagingFactory, WICBitmapEncoderNoCache,
        WICConvertBitmapSource, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        STREAM_SEEK_END, STREAM_SEEK_SET,
    };

    thread_local! {
        /// One imaging factory per thread (COM objects here aren't shared across threads).
        static FACTORY: RefCell<Option<IWICImagingFactory>> = const { RefCell::new(None) };
    }

    fn with_factory<T>(f: impl FnOnce(&IWICImagingFactory) -> Result<T>) -> Result<T> {
        FACTORY.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                // SAFETY: COM initialization for this thread (a no-op when already done),
                // then creating the in-process imaging factory.
                let made: Result<IWICImagingFactory> = unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                };
                *slot = Some(made?);
            }
            let factory = slot.as_ref().ok_or_else(|| Error::from(E_FAIL))?;
            f(factory)
        })
    }

    pub fn encode(bgra: &[u8], w: u32, h: u32) -> std::result::Result<Vec<u8>, String> {
        encode_com(bgra, w, h).map_err(|e| format!("jpeg encode: {e}"))
    }

    pub fn decode(jpeg: &[u8]) -> std::result::Result<(u32, u32, Vec<u8>), String> {
        decode_com(jpeg).map_err(|e| format!("jpeg decode: {e}"))
    }

    fn encode_com(bgra: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
        let bgr: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .take(w as usize * h as usize)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        with_factory(|f| {
            // SAFETY: WIC calls on live interfaces; `bgr` holds exactly `h` rows of `w * 3`
            // bytes, and the stream is read back only up to its own length.
            unsafe {
                let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
                let container = &GUID_ContainerFormatJpeg;
                let encoder = f.CreateEncoder(container, std::ptr::null())?;
                encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
                let mut frame = None;
                let mut options = None;
                encoder.CreateNewFrame(&mut frame, &mut options)?;
                let frame = frame.ok_or_else(|| Error::from(E_FAIL))?;
                frame.Initialize(options.as_ref())?;
                frame.SetSize(w, h)?;
                let mut format = GUID_WICPixelFormat24bppBGR;
                frame.SetPixelFormat(&mut format)?;
                if format != GUID_WICPixelFormat24bppBGR {
                    return Err(Error::from(E_FAIL));
                }
                frame.WritePixels(h, w * 3, &bgr)?;
                frame.Commit()?;
                encoder.Commit()?;
                let mut size = 0u64;
                stream.Seek(0, STREAM_SEEK_END, Some(std::ptr::addr_of_mut!(size)))?;
                stream.Seek(0, STREAM_SEEK_SET, None)?;
                let len = u32::try_from(size).map_err(|_| Error::from(E_FAIL))?;
                let mut out = vec![0u8; len as usize];
                let mut read = 0u32;
                let dst = out.as_mut_ptr().cast();
                let read_ptr = std::ptr::addr_of_mut!(read);
                stream.Read(dst, len, Some(read_ptr)).ok()?;
                out.truncate(read as usize);
                Ok(out)
            }
        })
    }

    fn decode_com(jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
        with_factory(|f| {
            // SAFETY: WIC calls on live interfaces; the stream reads `jpeg`, which outlives
            // it, and `CopyPixels` fills a buffer of exactly `h` rows of `w * 4` bytes.
            unsafe {
                let stream = f.CreateStream()?;
                stream.InitializeFromMemory(jpeg)?;
                let decoder = f.CreateDecoderFromStream(
                    &stream,
                    std::ptr::null(),
                    WICDecodeMetadataCacheOnDemand,
                )?;
                let frame = decoder.GetFrame(0)?;
                let bgra = WICConvertBitmapSource(&GUID_WICPixelFormat32bppBGRA, &frame)?;
                let (mut w, mut h) = (0u32, 0u32);
                bgra.GetSize(&mut w, &mut h)?;
                let mut px = vec![0u8; w as usize * h as usize * 4];
                bgra.CopyPixels(std::ptr::null(), w * 4, &mut px)?;
                Ok((w, h, px))
            }
        })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn encode(_bgra: &[u8], _w: u32, _h: u32) -> Result<Vec<u8>, String> {
        Err("JPEG encoding is Windows-only".into())
    }

    pub fn decode(_jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
        Err("JPEG decoding is Windows-only".into())
    }
}

/// Encode a BGRA frame (alpha ignored) as a JPEG.
///
/// # Errors
/// Returns a message if the frame is malformed or the encoder fails.
pub fn encode_bgra(bgra: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    if w == 0 || h == 0 || bgra.len() < w as usize * h as usize * 4 {
        return Err("camera frame has the wrong size".into());
    }
    imp::encode(bgra, w, h)
}

/// Decode a JPEG to `(width, height, BGRA pixels)`, alpha opaque.
///
/// # Errors
/// Returns a message if the bytes aren't a decodable image.
pub fn decode_bgra(jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    imp::decode(jpeg)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_frame_survives_the_round_trip() {
        let (w, h) = (64u32, 48u32);
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 4) as u8, (y * 5) as u8, 128, 255]
            })
            .collect();
        let jpeg = encode_bgra(&px, w, h).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "a JPEG starts with SOI");
        assert!(jpeg.len() < px.len());
        let (dw, dh, back) = decode_bgra(&jpeg).unwrap();
        assert_eq!((dw, dh), (w, h));
        let err: u64 = px
            .iter()
            .zip(&back)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = err as f64 / px.len() as f64;
        assert!(mean < 6.0, "mean error {mean}");
        assert!(back.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(decode_bgra(b"definitely not a jpeg").is_err());
        assert!(encode_bgra(&[0; 8], 4, 4).is_err());
    }
}
