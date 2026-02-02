//! # VTF (Valve Texture Format) Parser
//!
//! Parses VTF texture files used by Source engine and GoldSrc texture packs.
//!
//! ## VTF Format Overview
//!
//! VTF files contain:
//! - Header with format info, dimensions, flags
//! - Optional low-res thumbnail
//! - Mipmap chain (largest to smallest)
//! - Multiple image formats supported (DXT1, DXT5, RGBA8888, BGR888, etc.)
//!
//! ## Reference
//!
//! - https://developer.valvesoftware.com/wiki/VTF
//! - VTFLib source code

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::BspResult;
use crate::BspError;

// =============================================================================
// Constants
// =============================================================================

/// VTF file signature "VTF\0"
const VTF_SIGNATURE: [u8; 4] = [0x56, 0x54, 0x46, 0x00];

// Image format constants
const IMAGE_FORMAT_RGBA8888: i32 = 0;
const IMAGE_FORMAT_ABGR8888: i32 = 1;
const IMAGE_FORMAT_RGB888: i32 = 2;
const IMAGE_FORMAT_BGR888: i32 = 3;
const IMAGE_FORMAT_RGB565: i32 = 4;
const IMAGE_FORMAT_I8: i32 = 5;
const IMAGE_FORMAT_IA88: i32 = 6;
const IMAGE_FORMAT_A8: i32 = 8;
const IMAGE_FORMAT_RGB888_BLUESCREEN: i32 = 9;
const IMAGE_FORMAT_BGR888_BLUESCREEN: i32 = 10;
const IMAGE_FORMAT_ARGB8888: i32 = 11;
const IMAGE_FORMAT_BGRA8888: i32 = 12;
const IMAGE_FORMAT_DXT1: i32 = 13;
const IMAGE_FORMAT_DXT3: i32 = 14;
const IMAGE_FORMAT_DXT5: i32 = 15;
const IMAGE_FORMAT_BGRX8888: i32 = 16;
const IMAGE_FORMAT_BGR565: i32 = 17;
const IMAGE_FORMAT_BGRX5551: i32 = 18;
const IMAGE_FORMAT_BGRA4444: i32 = 19;
const IMAGE_FORMAT_BGRA5551: i32 = 21;
const IMAGE_FORMAT_UV88: i32 = 22;
const IMAGE_FORMAT_UVWQ8888: i32 = 23;
const IMAGE_FORMAT_RGBA16161616F: i32 = 24;
const IMAGE_FORMAT_RGBA16161616: i32 = 25;
const IMAGE_FORMAT_UVLX8888: i32 = 26;

// =============================================================================
// VTF Header Structure
// =============================================================================

/// VTF file header (version 7.0+)
#[derive(Debug, Clone)]
struct VtfHeader {
    signature: [u8; 4],
    version_major: u32,
    version_minor: u32,
    header_size: u32,
    width: u16,
    height: u16,
    flags: u32,
    frames: u16,
    first_frame: u16,
    reflectivity: [f32; 3],
    bumpmap_scale: f32,
    high_res_image_format: i32,
    mipmap_count: u8,
    low_res_image_format: i32,
    low_res_image_width: u8,
    low_res_image_height: u8,
    // Version 7.2+
    depth: u16,
}

// =============================================================================
// Public API
// =============================================================================

/// Parsed VTF texture data
#[derive(Debug, Clone)]
pub struct VtfTexture {
    /// Texture name (from filename)
    pub name: String,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// RGB pixel data (width * height * 3 bytes)
    pub pixels: Vec<u8>,
}

/// Load a VTF texture file and convert to RGB pixels.
pub fn load_vtf<P: AsRef<Path>>(path: P) -> BspResult<VtfTexture> {
    let path = path.as_ref();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read and validate signature
    let mut signature = [0u8; 4];
    reader.read_exact(&mut signature)?;
    if signature != VTF_SIGNATURE {
        return Err(BspError::InvalidFormat("Not a VTF file".to_string()));
    }

    // Read version
    let mut version_buf = [0u8; 8];
    reader.read_exact(&mut version_buf)?;
    let version_major = u32::from_le_bytes([version_buf[0], version_buf[1], version_buf[2], version_buf[3]]);
    let version_minor = u32::from_le_bytes([version_buf[4], version_buf[5], version_buf[6], version_buf[7]]);

    // Read header size
    let mut header_size_buf = [0u8; 4];
    reader.read_exact(&mut header_size_buf)?;
    let header_size = u32::from_le_bytes(header_size_buf);

    // Read dimensions
    let mut dim_buf = [0u8; 4];
    reader.read_exact(&mut dim_buf)?;
    let width = u16::from_le_bytes([dim_buf[0], dim_buf[1]]) as u32;
    let height = u16::from_le_bytes([dim_buf[2], dim_buf[3]]) as u32;

    // Read flags
    let mut flags_buf = [0u8; 4];
    reader.read_exact(&mut flags_buf)?;
    let _flags = u32::from_le_bytes(flags_buf);

    // Read frames
    let mut frames_buf = [0u8; 4];
    reader.read_exact(&mut frames_buf)?;
    let _frames = u16::from_le_bytes([frames_buf[0], frames_buf[1]]);
    let _first_frame = u16::from_le_bytes([frames_buf[2], frames_buf[3]]);

    // Skip padding (4 bytes)
    reader.seek(SeekFrom::Current(4))?;

    // Read reflectivity (12 bytes)
    reader.seek(SeekFrom::Current(12))?;

    // Skip padding (4 bytes)
    reader.seek(SeekFrom::Current(4))?;

    // Read bumpmap scale
    reader.seek(SeekFrom::Current(4))?;

    // Read high-res image format
    let mut format_buf = [0u8; 4];
    reader.read_exact(&mut format_buf)?;
    let image_format = i32::from_le_bytes(format_buf);

    // Read mipmap count
    let mut mipmap_buf = [0u8; 1];
    reader.read_exact(&mut mipmap_buf)?;
    let mipmap_count = mipmap_buf[0];

    // Read low-res image format
    let mut low_format_buf = [0u8; 4];
    reader.read_exact(&mut low_format_buf)?;
    let low_res_format = i32::from_le_bytes(low_format_buf);

    // Read low-res dimensions
    let mut low_dim_buf = [0u8; 2];
    reader.read_exact(&mut low_dim_buf)?;
    let low_res_width = low_dim_buf[0] as u32;
    let low_res_height = low_dim_buf[1] as u32;

    // Seek to start of image data (after header)
    reader.seek(SeekFrom::Start(header_size as u64))?;

    // Skip low-res image if present
    if low_res_format != -1 && low_res_width > 0 && low_res_height > 0 {
        let low_res_size = compute_image_size(low_res_format, low_res_width, low_res_height);
        reader.seek(SeekFrom::Current(low_res_size as i64))?;
    }

    // Calculate offset to the largest mipmap (mip 0)
    // Mipmaps are stored smallest to largest, so we need to skip smaller ones
    let mut offset: u64 = 0;
    for mip in (1..mipmap_count).rev() {
        let mip_width = (width >> mip).max(1);
        let mip_height = (height >> mip).max(1);
        let mip_size = compute_image_size(image_format, mip_width, mip_height);
        offset += mip_size as u64;
    }
    reader.seek(SeekFrom::Current(offset as i64))?;

    // Read the largest mipmap
    let image_size = compute_image_size(image_format, width, height);
    let mut image_data = vec![0u8; image_size];
    reader.read_exact(&mut image_data)?;

    // Convert to RGB
    let pixels = convert_to_rgb(&image_data, image_format, width, height)?;

    Ok(VtfTexture {
        name,
        width,
        height,
        pixels,
    })
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Compute the size in bytes of an image with the given format and dimensions.
fn compute_image_size(format: i32, width: u32, height: u32) -> usize {
    let pixels = (width * height) as usize;
    
    match format {
        IMAGE_FORMAT_RGBA8888 | IMAGE_FORMAT_ABGR8888 | IMAGE_FORMAT_ARGB8888 |
        IMAGE_FORMAT_BGRA8888 | IMAGE_FORMAT_BGRX8888 | IMAGE_FORMAT_UVWQ8888 |
        IMAGE_FORMAT_UVLX8888 => pixels * 4,
        
        IMAGE_FORMAT_RGB888 | IMAGE_FORMAT_BGR888 | IMAGE_FORMAT_RGB888_BLUESCREEN |
        IMAGE_FORMAT_BGR888_BLUESCREEN => pixels * 3,
        
        IMAGE_FORMAT_RGB565 | IMAGE_FORMAT_BGR565 | IMAGE_FORMAT_BGRX5551 |
        IMAGE_FORMAT_BGRA5551 | IMAGE_FORMAT_BGRA4444 | IMAGE_FORMAT_IA88 |
        IMAGE_FORMAT_UV88 => pixels * 2,
        
        IMAGE_FORMAT_I8 | IMAGE_FORMAT_A8 => pixels,
        
        IMAGE_FORMAT_DXT1 => {
            // DXT1: 4x4 blocks, 8 bytes per block
            let blocks_x = (width + 3) / 4;
            let blocks_y = (height + 3) / 4;
            (blocks_x * blocks_y * 8) as usize
        }
        
        IMAGE_FORMAT_DXT3 | IMAGE_FORMAT_DXT5 => {
            // DXT3/DXT5: 4x4 blocks, 16 bytes per block
            let blocks_x = (width + 3) / 4;
            let blocks_y = (height + 3) / 4;
            (blocks_x * blocks_y * 16) as usize
        }
        
        IMAGE_FORMAT_RGBA16161616F | IMAGE_FORMAT_RGBA16161616 => pixels * 8,
        
        _ => pixels * 4, // Default to 4 bytes per pixel
    }
}

/// Convert image data from VTF format to RGB.
fn convert_to_rgb(data: &[u8], format: i32, width: u32, height: u32) -> BspResult<Vec<u8>> {
    let pixels = (width * height) as usize;
    let mut rgb = Vec::with_capacity(pixels * 3);

    match format {
        IMAGE_FORMAT_RGB888 => {
            rgb.extend_from_slice(data);
        }
        
        IMAGE_FORMAT_BGR888 | IMAGE_FORMAT_BGR888_BLUESCREEN => {
            for chunk in data.chunks(3) {
                if chunk.len() >= 3 {
                    rgb.push(chunk[2]); // R
                    rgb.push(chunk[1]); // G
                    rgb.push(chunk[0]); // B
                }
            }
        }
        
        IMAGE_FORMAT_RGBA8888 => {
            for chunk in data.chunks(4) {
                if chunk.len() >= 3 {
                    rgb.push(chunk[0]); // R
                    rgb.push(chunk[1]); // G
                    rgb.push(chunk[2]); // B
                }
            }
        }
        
        IMAGE_FORMAT_BGRA8888 | IMAGE_FORMAT_BGRX8888 => {
            for chunk in data.chunks(4) {
                if chunk.len() >= 3 {
                    rgb.push(chunk[2]); // R
                    rgb.push(chunk[1]); // G
                    rgb.push(chunk[0]); // B
                }
            }
        }
        
        IMAGE_FORMAT_ARGB8888 => {
            for chunk in data.chunks(4) {
                if chunk.len() >= 4 {
                    rgb.push(chunk[1]); // R
                    rgb.push(chunk[2]); // G
                    rgb.push(chunk[3]); // B
                }
            }
        }
        
        IMAGE_FORMAT_ABGR8888 => {
            for chunk in data.chunks(4) {
                if chunk.len() >= 4 {
                    rgb.push(chunk[3]); // R
                    rgb.push(chunk[2]); // G
                    rgb.push(chunk[1]); // B
                }
            }
        }
        
        IMAGE_FORMAT_DXT1 => {
            decode_dxt1(data, width, height, &mut rgb);
        }
        
        IMAGE_FORMAT_DXT5 => {
            decode_dxt5(data, width, height, &mut rgb);
        }
        
        IMAGE_FORMAT_I8 => {
            // Grayscale
            for &byte in data {
                rgb.push(byte);
                rgb.push(byte);
                rgb.push(byte);
            }
        }
        
        _ => {
            // Unsupported format - return magenta
            for _ in 0..pixels {
                rgb.push(255);
                rgb.push(0);
                rgb.push(255);
            }
        }
    }

    Ok(rgb)
}

/// Decode DXT1 compressed texture to RGB.
fn decode_dxt1(data: &[u8], width: u32, height: u32, rgb: &mut Vec<u8>) {
    let blocks_x = (width + 3) / 4;
    let blocks_y = (height + 3) / 4;
    
    // Initialize output buffer
    rgb.resize((width * height * 3) as usize, 0);
    
    let mut block_idx = 0;
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let block_offset = block_idx * 8;
            if block_offset + 8 > data.len() {
                break;
            }
            
            // Read color endpoints
            let c0 = u16::from_le_bytes([data[block_offset], data[block_offset + 1]]);
            let c1 = u16::from_le_bytes([data[block_offset + 2], data[block_offset + 3]]);
            
            // Decode 565 colors
            let colors = decode_dxt_colors(c0, c1);
            
            // Read 4x4 pixel indices (2 bits each, 32 bits total)
            let indices = u32::from_le_bytes([
                data[block_offset + 4],
                data[block_offset + 5],
                data[block_offset + 6],
                data[block_offset + 7],
            ]);
            
            // Write pixels
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    
                    if x < width && y < height {
                        let pixel_idx = (py * 4 + px) as u32;
                        let color_idx = ((indices >> (pixel_idx * 2)) & 0x3) as usize;
                        let color = &colors[color_idx];
                        
                        let out_idx = ((y * width + x) * 3) as usize;
                        rgb[out_idx] = color[0];
                        rgb[out_idx + 1] = color[1];
                        rgb[out_idx + 2] = color[2];
                    }
                }
            }
            
            block_idx += 1;
        }
    }
}

/// Decode DXT5 compressed texture to RGB.
fn decode_dxt5(data: &[u8], width: u32, height: u32, rgb: &mut Vec<u8>) {
    let blocks_x = (width + 3) / 4;
    let blocks_y = (height + 3) / 4;
    
    // Initialize output buffer
    rgb.resize((width * height * 3) as usize, 0);
    
    let mut block_idx = 0;
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let block_offset = block_idx * 16;
            if block_offset + 16 > data.len() {
                break;
            }
            
            // Skip alpha block (8 bytes) - we only need RGB
            let color_offset = block_offset + 8;
            
            // Read color endpoints
            let c0 = u16::from_le_bytes([data[color_offset], data[color_offset + 1]]);
            let c1 = u16::from_le_bytes([data[color_offset + 2], data[color_offset + 3]]);
            
            // Decode 565 colors
            let colors = decode_dxt_colors(c0, c1);
            
            // Read 4x4 pixel indices
            let indices = u32::from_le_bytes([
                data[color_offset + 4],
                data[color_offset + 5],
                data[color_offset + 6],
                data[color_offset + 7],
            ]);
            
            // Write pixels
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    
                    if x < width && y < height {
                        let pixel_idx = (py * 4 + px) as u32;
                        let color_idx = ((indices >> (pixel_idx * 2)) & 0x3) as usize;
                        let color = &colors[color_idx];
                        
                        let out_idx = ((y * width + x) * 3) as usize;
                        rgb[out_idx] = color[0];
                        rgb[out_idx + 1] = color[1];
                        rgb[out_idx + 2] = color[2];
                    }
                }
            }
            
            block_idx += 1;
        }
    }
}

/// Decode DXT color endpoints and interpolate.
fn decode_dxt_colors(c0: u16, c1: u16) -> [[u8; 3]; 4] {
    // Decode RGB565 to RGB888
    let r0 = ((c0 >> 11) & 0x1F) as u8;
    let g0 = ((c0 >> 5) & 0x3F) as u8;
    let b0 = (c0 & 0x1F) as u8;
    
    let r1 = ((c1 >> 11) & 0x1F) as u8;
    let g1 = ((c1 >> 5) & 0x3F) as u8;
    let b1 = (c1 & 0x1F) as u8;
    
    // Expand to 8-bit
    let color0 = [
        (r0 << 3) | (r0 >> 2),
        (g0 << 2) | (g0 >> 4),
        (b0 << 3) | (b0 >> 2),
    ];
    let color1 = [
        (r1 << 3) | (r1 >> 2),
        (g1 << 2) | (g1 >> 4),
        (b1 << 3) | (b1 >> 2),
    ];
    
    // Interpolate
    let color2 = if c0 > c1 {
        [
            ((2 * color0[0] as u16 + color1[0] as u16) / 3) as u8,
            ((2 * color0[1] as u16 + color1[1] as u16) / 3) as u8,
            ((2 * color0[2] as u16 + color1[2] as u16) / 3) as u8,
        ]
    } else {
        [
            ((color0[0] as u16 + color1[0] as u16) / 2) as u8,
            ((color0[1] as u16 + color1[1] as u16) / 2) as u8,
            ((color0[2] as u16 + color1[2] as u16) / 2) as u8,
        ]
    };
    
    let color3 = if c0 > c1 {
        [
            ((color0[0] as u16 + 2 * color1[0] as u16) / 3) as u8,
            ((color0[1] as u16 + 2 * color1[1] as u16) / 3) as u8,
            ((color0[2] as u16 + 2 * color1[2] as u16) / 3) as u8,
        ]
    } else {
        [0, 0, 0] // Transparent black for DXT1
    };
    
    [color0, color1, color2, color3]
}

/// Search for a texture by name in a directory of VTF files.
/// Returns the loaded texture if found.
pub fn find_texture<P: AsRef<Path>>(texture_name: &str, search_dir: P) -> Option<VtfTexture> {
    let search_dir = search_dir.as_ref();
    
    // Try exact match first
    let vtf_path = search_dir.join(format!("{}.vtf", texture_name));
    if vtf_path.exists() {
        if let Ok(tex) = load_vtf(&vtf_path) {
            return Some(tex);
        }
    }
    
    // Try case-insensitive search in subdirectories
    let lower_name = texture_name.to_lowercase();
    
    for subdir in &["halflife", "decals", "liquids", ""] {
        let dir = if subdir.is_empty() {
            search_dir.to_path_buf()
        } else {
            search_dir.join(subdir)
        };
        
        if !dir.exists() {
            continue;
        }
        
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "vtf").unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if stem.to_lowercase() == lower_name {
                            if let Ok(tex) = load_vtf(&path) {
                                return Some(tex);
                            }
                        }
                    }
                }
            }
        }
    }
    
    None
}
