/*
code inspired from: git@github.com:y-stm/rust-iconv.git
*/
use std::borrow::Cow;
use errno::{Errno, errno};
use std;
use std::ffi::CString;
use std::fmt::{Display, Formatter};
use std::error::Error;
use std::mem::transmute;
use libc::size_t;
use libc;
#[allow(unused_imports)]
use env_logger;

#[link(name="iconv")]
pub mod raw {
    use libc::{size_t, c_char, c_void, c_int};
    #[allow(non_camel_case_types)]
    pub type iconv_t = *mut c_void;

    /// Check whether iconv_t is successfully created
    ///
    /// `iconv_open(3)` returns (iconv_t)-1 when failed to create new iconv object.
    /// In rust we should use `std::mem::transmute` to check the return value.
    #[inline]
    pub fn is_iconv_t_valid(cd: &iconv_t) -> bool {
        unsafe {
            let err = ::std::mem::transmute::<isize, iconv_t>(-1);
            return cd != &err;
        }
    }

    /// Check wheter iconv conversion is successfully done.
    ///
    /// `iconv(3)` returns (size_t)-1 when conversion failed.
    /// In rust we should use unsafe `std::mem::transmute` to check the return value.
    #[inline]
    pub fn is_iconv_valid(v: &size_t) -> bool {
        unsafe {
            let err = ::std::mem::transmute::<isize, size_t>(-1);
            return v != &err;
        }
    }

    extern {
        pub fn iconv(cd: iconv_t,
                     inbuf: *const (*const u8),
                     inbytesleft: *mut size_t,
                     outbuf: *mut (*mut u8),
                     outbytesleft: *mut size_t)
                     -> size_t;
        pub fn iconv_open(tocode: *const c_char, fromcode: *const c_char) -> iconv_t;
        pub fn iconv_close(cd: iconv_t) -> c_int;
    }

}

/// The error given by iconv.
///
/// This include the below:
/// * Input/output buffer has no sufficient room during conversion.
/// * Failed to create iconv object
/// * Failed to make CString of encoding name (at `Iconv::new`)
#[derive(Debug)]
pub enum IconvError {
    OnFindingConversion(Errno),
    OnCStringConversion(std::ffi::NulError),
    OnConversion(Cow<'static, str>),
    InvalidSequence(usize, usize),
    InsufficientOutBuffer(usize, usize),
    InsufficientInBuffer(usize, usize),
    OnClose(Errno),
}

impl IconvError {
    pub fn iconv_err(e: Errno) -> Self {
        IconvError::OnFindingConversion(e)
    }
    pub fn cstring_err(e: std::ffi::NulError) -> Self {
        IconvError::OnCStringConversion(e)
    }

    /// Returns short description of `IconvError`
    ///
    /// This is used in `(IconvError as Display)::fmt`
    pub fn to_str(&self) -> String {
        use self::IconvError::*;
        match self {
            &OnFindingConversion(ref e) => {
                format!("C function `iconv_open` failed: {}", e)
            }
            &OnCStringConversion(ref e) => {
                format!("CString::new failed: {}", e)
            }
            &OnConversion(ref cow_str) => {
                cow_str.to_owned().to_string()
            }
            &InsufficientOutBuffer(ref left_to_convert, ref wrote_bytes) => {
                format!("Need more room in dst buffer. {} bytes remain. {} bytes written",
                        left_to_convert,
                        wrote_bytes)
            }
            &InsufficientInBuffer(ref remain_index, ref wrote_index) => {
                format!("Need more input to complete conversion. {} bytes remain, {} bytes written",
                        remain_index,
                        wrote_index)
            }
            &InvalidSequence(ref remain_index, ref wrote_index) => {
                format!("Source text has invalid multibyte charactor sequence at {}. {} bytes \
                         written",
                        remain_index,
                        wrote_index)
            }
            &OnClose(ref e) => {
                format!("C function `iconv_close` failed: {}", e)
            }
        }
    }
}

impl Display for IconvError {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), std::fmt::Error> {
        fmt.write_str(&self.to_str())
    }
}

impl Error for IconvError {
    fn description(&self) -> &str {
        "Error happend using Iconv"
    }
}

pub struct Iconv {
    iconv: raw::iconv_t,
}

impl Iconv {
    pub fn new(tocode: &str, fromcode: &str) -> Result<Iconv, IconvError> {
        let to_iconv_err = |null_err| {
            return Err(IconvError::OnCStringConversion(null_err));
        };
        let tocode_c = try!(CString::new(tocode).or_else(&to_iconv_err));
        let fromcode_c = try!(CString::new(fromcode).or_else(&to_iconv_err));
        unsafe {

            let cd = raw::iconv_open(tocode_c.into_raw(), fromcode_c.into_raw());
            if !raw::is_iconv_t_valid(&cd) {
                return Err(IconvError::OnFindingConversion(errno()));
            }
            return Ok(Iconv { iconv: cd });
        }
    }

    /// Convert src sequence into anotherencoding.
    ///
    /// Convert src sequence into another encoding, putting them into dst buffer from start.
    /// # Arguments
    /// * `src` - The input text (&[u8] encoded in "from encoding")
    /// * `dst` - Where the converted text is stored.
    /// # Returns
    /// * `Result::Ok(usize)` - When all text in `src` are converted successfully,
    ///                         it returns the bytes of written to `dst`
    /// * `Result::Err(InsufficientOutBuffer(usize, usize)) -
    ///         If there's no sufficient room in `dst`, returns the index of `src` where conversion
    ///         is processed and the index of `dst` where the converted text is written.
    pub fn convert_raw(&mut self, src: &[u8], mut dst: &mut [u8]) -> Result<usize, IconvError> {
        use self::IconvError::*;
        let mut inbytes_left = src.len();
        let mut outbytes_left = dst.len();
        unsafe {
            let inbytes_left = &mut inbytes_left as *mut size_t;
            let outbytes_left = &mut outbytes_left as *mut size_t;
            let unsafe_src = transmute::<&&[u8], *const (*const u8)>(&src);
            let unsafe_dst = transmute::<&mut &mut [u8], *mut (*mut u8)>(&mut dst);
            let res = raw::iconv(self.iconv,
                                 unsafe_src,
                                 inbytes_left,
                                 unsafe_dst,
                                 outbytes_left);
            info!("inbytes_left:{}, outbytes_left:{}, res:{}",
                  *inbytes_left,
                  *outbytes_left,
                  res);
            if !raw::is_iconv_valid(&res) {
                let Errno(err_num) = errno();
                match err_num {
                    libc::E2BIG => {
                        info!("output buffer has no sufficient room");
                        return Err(InsufficientOutBuffer(src.len() - (*inbytes_left as usize),
                                                         dst.len() - (*outbytes_left as usize)));
                    }
                    libc::EILSEQ => {
                        info!("Invalid multibyte in input");
                        return Err(InvalidSequence(src.len() - (*inbytes_left as usize),
                                                   dst.len() - (*outbytes_left as usize)));
                    }
                    libc::EINVAL => {
                        info!("Insufficient multibyte in input. Maybe more input is needed.");
                        return Err(InsufficientInBuffer(src.len() - (*inbytes_left as usize),
                                                        dst.len() - (*outbytes_left as usize)));
                    }
                    _ => {
                        info!("Unknown error");
                        return Err(OnConversion(Cow::Borrowed("Unknown Error")));
                    }
                }
            }
            return Ok(dst.len() - (*outbytes_left as usize));
        }
    }

    /// Converts text into another encoding.
    ///
    /// #Arguments
    /// * `src` - The input text
    /// * `dst` - The output text encoded in this struct.
    /// * `start_index` - The index of `dst` where output will be written.
    ///
    /// #Returns
    /// * Ok(unit) - Returns when all text in `src` has converted into `dst`
    /// * IconvError(InsufficientInBuffer(usize, usize))
    ///     - Returns when `src` has incomplete sequence.
    ///       It may be recovered by adding more sequence of input.
    /// * IconvError(InvalidSequence(usize, usize))
    ///     - Returns when `src` has invalid multibyte sequence.
    /// * IconvError(OnConversion(_)) - Something wrong happend due to any other reason
    pub fn convert(&mut self,
                   src: &[u8],
                   dst: &mut Vec<u8>,
                   start_index: usize)
                   -> Result<(), IconvError> {
        let mut src_index = 0;
        let mut dst_index = start_index;
        loop {
            match self.convert_raw(&src[src_index..], &mut dst[dst_index..]) {
                Ok(written) => {
                    let new_length = written + dst_index;
                    dst.truncate(new_length);
                    return Ok(());
                }
                Err(IconvError::InsufficientOutBuffer(left_index, wrote_index)) => {
                    let room = ((src.len() - left_index) * 2) + 10;
                    dst.reserve(room);
                    unsafe {
                        dst.set_len(room + dst_index);
                    }
                    src_index = left_index;
                    dst_index = wrote_index;
                    continue;
                }
                err => {
                    return err.and(Ok(()));
                }
            }
        }
    }
}

impl Drop for Iconv {
    fn drop(&mut self) {
        unsafe {
            if raw::iconv_close(self.iconv) == -1 {
                info!("Error on disposing iconv descriptor: {}", errno());
            }
        }
    }
}

#[test]
fn test_iconv_raw() {
    let _ = env_logger::init();
    let mut iconv = Iconv::new("cp932", "utf-8").unwrap();
    let mut src = "あいうえお".bytes().collect::<Vec<u8>>();
    let mut outbuf = [0u8; 1000];
    let res = iconv.convert_raw(&mut src, &mut outbuf).unwrap();
    info!("First result: {}", res);
    info!("CP932 converted: {:?}", &outbuf[0..11]);
    let mut outbuf2 = [0u8; 1000];
    let mut iconv = Iconv::new("utf-8", "cp932").unwrap();
    let res2 = iconv.convert_raw(&mut outbuf[0..res], &mut outbuf2).unwrap();
    info!("Second result: {}", res2);
    let s = String::from_utf8_lossy(&outbuf2[0..res2]);
    info!("Recoverd: {}", s);
    assert_eq!(&s, "あいうえお");

}

/// Confirm that `Iconv::convert_raw` set errno to E2BIG when output buffer has no room
#[test]
fn what_if_dst_array_is_short() {
    let _ = env_logger::init();
    let mut iconv = Iconv::new("cp932", "utf-8").unwrap();
    let mut src = "あいうえお".bytes().collect::<Vec<u8>>();
    let mut outbuf = [0u8; 4];
    info!("Let's begin shortcomming\n");
    if let Err(IconvError::InsufficientOutBuffer(_, _)) = iconv.convert_raw(&mut src, &mut outbuf) {
        return;
    } else {
        unreachable!();
    }

}

/// Test `Iconv::convert` gives the same result as the input through utf-8 -> cp932 -> utf-8
/// conversions
#[test]
fn test_convert_raw_turn() {
    let _ = env_logger::init();
    let mut iconv = Iconv::new("cp932", "utf-8").unwrap();
    let mut iconv_rev = Iconv::new("utf-8", "cp932").unwrap();
    let mut src = "あいうえお".bytes().collect::<Vec<u8>>();
    let mut dst = Vec::new();
    let res = iconv.convert(&mut src, &mut dst, 0);
    match res {
        Ok(_) => {}
        Err(_) => {
            panic!("Conversion failed");
        }
    }
    info!("line {}: Result of convert: {:?}", line!(), dst);
    let mut dst2 = Vec::new();
    let _ = iconv_rev.convert(&mut dst, &mut dst2, 0);
    let s_recoverd = String::from_utf8(dst2).unwrap();
    assert_eq!("あいうえお".to_string(), s_recoverd);
}
