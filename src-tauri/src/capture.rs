//! 屏幕捕获：GDI BitBlt 抓屏 → 缩放 → JPEG
use base64::Engine;
use windows_sys::Win32::Graphics::Gdi::{
  BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetDIBits,
  ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};

/// 抓取主屏 BGRA 像素（top-down）
fn capture_bgra() -> Result<(Vec<u8>, i32, i32), String> {
  unsafe {
    let screen_dc = GetDC(std::ptr::null_mut());
    if screen_dc.is_null() {
      return Err("GetDC failed".into());
    }
    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.is_null() {
      ReleaseDC(std::ptr::null_mut(), screen_dc);
      return Err("CreateCompatibleDC failed".into());
    }

    // 主屏尺寸
    let w = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0); // SM_CXSCREEN
    let h = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1); // SM_CYSCREEN
    if w <= 0 || h <= 0 {
      DeleteDC(mem_dc);
      ReleaseDC(std::ptr::null_mut(), screen_dc);
      return Err("GetSystemMetrics failed".into());
    }

    let mut bi = BITMAPINFO {
      bmiHeader: BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
      },
      bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bmp = CreateDIBSection(mem_dc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
      DeleteDC(mem_dc);
      ReleaseDC(std::ptr::null_mut(), screen_dc);
      return Err("CreateDIBSection failed".into());
    }
    let old = SelectObject(mem_dc, bmp);
    let ok = BitBlt(mem_dc, 0, 0, w, h, screen_dc, 0, 0, SRCCOPY);
    SelectObject(mem_dc, old);

    let stride = (w as usize) * 4;
    let mut out = vec![0u8; stride * (h as usize)];
    let mut result: Result<(), String> = Ok(());
    if ok == 0 {
      result = Err("BitBlt failed".into());
    } else {
      let needed = GetDIBits(
        mem_dc,
        bmp,
        0,
        h as u32,
        out.as_mut_ptr() as *mut std::ffi::c_void,
        &mut bi,
        DIB_RGB_COLORS,
      );
      if needed == 0 {
        result = Err("GetDIBits failed".into());
      }
    }
    DeleteObject(bmp);
    DeleteDC(mem_dc);
    ReleaseDC(std::ptr::null_mut(), screen_dc);
    result?;
    Ok((out, w, h))
  }
}

/// 简单抽点缩放（每 step 像素取 1），返回 BGRA
fn downscale(data: &[u8], w: i32, h: i32, step: i32) -> (Vec<u8>, i32, i32) {
  let nw = (w / step).max(1);
  let nh = (h / step).max(1);
  let stride = (w as usize) * 4;
  let mut out = vec![0u8; (nw as usize) * 4 * (nh as usize)];
  for y in 0..nh {
    for x in 0..nw {
      let sx = (x * step) as usize;
      let sy = (y * step) as usize;
      let s = sy * stride + sx * 4;
      let d = ((y * nw + x) as usize) * 4;
      out[d..d + 4].copy_from_slice(&data[s..s + 4]);
    }
  }
  (out, nw, nh)
}

fn encode_jpeg(bgra: &[u8], w: i32, h: i32, quality: u8) -> Result<Vec<u8>, String> {
  // BGRA → RGB
  let px = (w as usize) * (h as usize);
  let mut rgb = Vec::with_capacity(px * 3);
  for i in 0..px {
    let s = i * 4;
    rgb.push(bgra[s + 2]);
    rgb.push(bgra[s + 1]);
    rgb.push(bgra[s]);
  }
  let mut out = Vec::new();
  let enc = jpeg_encoder::Encoder::new(&mut out, quality);
  enc
    .encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
    .map_err(|e| e.to_string())?;
  Ok(out)
}

/// 抓屏并编码为 JPEG（scale_step: 缩放步长，1=原始）
pub fn grab_jpeg(scale_step: i32, quality: u8) -> Result<Vec<u8>, String> {
  let (bgra, w, h) = capture_bgra()?;
  let (data, w, h) = if scale_step > 1 {
    downscale(&bgra, w, h, scale_step)
  } else {
    (bgra, w, h)
  };
  encode_jpeg(&data, w, h, quality)
}

/// 抓屏 JPEG 的 base64
pub fn grab_jpeg_base64(scale_step: i32, quality: u8) -> Result<String, String> {
  let jpg = grab_jpeg(scale_step, quality)?;
  Ok(base64::engine::general_purpose::STANDARD.encode(jpg))
}

/// 远程鼠标左键点击（主屏绝对坐标）
pub fn send_click(x: i32, y: i32) -> Result<(), String> {
  use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEINPUT,
  };
  unsafe {
    let sw = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0).max(1);
    let sh = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1).max(1);
    let ax = ((x.clamp(0, sw) as i64) * 65535 / (sw as i64)) as i32;
    let ay = ((y.clamp(0, sh) as i64) * 65535 / (sh as i64)) as i32;
    let mut inputs: [INPUT; 3] = std::mem::zeroed();
    for inp in &mut inputs {
      inp.r#type = INPUT_MOUSE;
    }
    inputs[0].Anonymous.mi = MOUSEINPUT {
      dx: ax,
      dy: ay,
      mouseData: 0,
      dwFlags: MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
      time: 0,
      dwExtraInfo: 0,
    };
    inputs[1].Anonymous.mi = MOUSEINPUT {
      dx: 0,
      dy: 0,
      mouseData: 0,
      dwFlags: MOUSEEVENTF_LEFTDOWN,
      time: 0,
      dwExtraInfo: 0,
    };
    inputs[2].Anonymous.mi = MOUSEINPUT {
      dx: 0,
      dy: 0,
      mouseData: 0,
      dwFlags: MOUSEEVENTF_LEFTUP,
      time: 0,
      dwExtraInfo: 0,
    };
    let n = SendInput(3, inputs.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
    if n != 3 {
      return Err("SendInput 失败".into());
    }
    Ok(())
  }
}

/// 远程键盘输入（Unicode 逐字符注入，支持中文）
pub fn send_text(s: &str) -> Result<(), String> {
  use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, KEYBDINPUT,
  };
  if s.is_empty() || s.chars().count() > 200 {
    return Err("输入内容无效".into());
  }
  let mut inputs: Vec<INPUT> = Vec::new();
  for ch in s.chars() {
    let code: u16 = if ch == '\n' { 0x0D } else { ch as u32 as u16 };
    unsafe {
      let mut down: INPUT = std::mem::zeroed();
      down.r#type = INPUT_KEYBOARD;
      down.Anonymous.ki = KEYBDINPUT {
        wVk: 0,
        wScan: code,
        dwFlags: KEYEVENTF_UNICODE,
        time: 0,
        dwExtraInfo: 0,
      };
      let mut up: INPUT = std::mem::zeroed();
      up.r#type = INPUT_KEYBOARD;
      up.Anonymous.ki = KEYBDINPUT {
        wVk: 0,
        wScan: code,
        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        time: 0,
        dwExtraInfo: 0,
      };
      inputs.push(down);
      inputs.push(up);
    }
  }
  unsafe {
    let n = SendInput(
      inputs.len() as u32,
      inputs.as_mut_ptr(),
      std::mem::size_of::<INPUT>() as i32,
    );
    if n != inputs.len() as u32 {
      return Err("SendInput 失败".into());
    }
  }
  Ok(())
}
