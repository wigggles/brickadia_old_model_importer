use crate::geometry::{interpolate_uv, intersect};
use crate::color::utils::*;
use crate::app::{BrickType, Logger};
use super::octree::{Branches, TreeBody, VoxelTree};

use cgmath::{Vector2, Vector3, Vector4};
use image::RgbaImage;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Axis-Aligned Bounding Box for fast rejection tests
#[derive(Debug, Copy, Clone)]
struct AABB {
    min: Vector3<f32>,
    max: Vector3<f32>,
}

impl AABB {
    /// Create AABB from triangle vertices
    fn from_triangle(v0: Vector3<f32>, v1: Vector3<f32>, v2: Vector3<f32>) -> Self {
        Self {
            min: Vector3::new(
                v0.x.min(v1.x).min(v2.x),
                v0.y.min(v1.y).min(v2.y),
                v0.z.min(v1.z).min(v2.z),
            ),
            max: Vector3::new(
                v0.x.max(v1.x).max(v2.x),
                v0.y.max(v1.y).max(v2.y),
                v0.z.max(v1.z).max(v2.z),
            ),
        }
    }

    /// Fast test: does this AABB intersect a cube centered at `center` with half-extent `half_box`?
    #[inline(always)]
    fn intersects_cube(&self, center: Vector3<f32>, half_box: f32) -> bool {
        // Check if AABB overlaps with the cube on all three axes
        let cube_min = center - Vector3::new(half_box, half_box, half_box);
        let cube_max = center + Vector3::new(half_box, half_box, half_box);
        
        self.max.x >= cube_min.x && self.min.x <= cube_max.x &&
        self.max.y >= cube_min.y && self.min.y <= cube_max.y &&
        self.max.z >= cube_min.z && self.min.z <= cube_max.z
    }

    /// Get the maximum dimension of the AABB (for filtering small triangles)
    fn max_dimension(&self) -> f32 {
        let size = self.max - self.min;
        size.x.max(size.y).max(size.z)
    }

    /// Translate the AABB by subtracting an offset (for recursive subdivision)
    fn translated(&self, offset: Vector3<f32>) -> Self {
        Self {
            min: self.min - offset,
            max: self.max - offset,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Triangle {
    material_id: Option<usize>,
    vertices: [Vector3<f32>; 3],
    uvs: Option<[Vector2<f32>; 3]>,
    /// Pre-computed bounding box for fast rejection
    aabb: AABB,
}

/// Bounding box for a set of triangles
#[derive(Debug, Clone, Copy)]
pub struct MaterialBounds {
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
}

/// Pre-extracted triangles grouped by material ID for efficient multi-material processing.
/// This avoids re-parsing models for each material.
pub struct PreGroupedTriangles {
    /// Triangles grouped by material ID (None key = no material)
    pub by_material: HashMap<Option<usize>, Vec<Triangle>>,
    /// Per-material bounding boxes for efficient octree sizing
    pub bounds_by_material: HashMap<Option<usize>, MaterialBounds>,
    /// Global bounding box for all triangles (kept for potential future alignment features)
    #[allow(dead_code)]
    pub global_min: Vector3<f32>,
    #[allow(dead_code)]
    pub global_max: Vector3<f32>,
    /// Total triangle count
    pub total_count: usize,
}

impl PreGroupedTriangles {
    /// Extract all triangles from models and group them by material ID.
    /// This is done once, then individual materials can be voxelized efficiently.
    pub fn from_models(models: &[tobj::Model]) -> Self {
        let mut by_material: HashMap<Option<usize>, Vec<Triangle>> = HashMap::new();
        let mut total_count = 0;
        
        // Initialize global bounds from first vertex
        let u = &models[0].mesh.positions;
        let mut global_min = Vector3::new(u[0], u[1], u[2]);
        let mut global_max = global_min;
        
        // First pass: compute global bounds
        for m in models.iter() {
            let p = &m.mesh.positions;
            for v in (0..p.len()).step_by(3) {
                for axis in 0..3 {
                    global_min[axis] = global_min[axis].min(p[v + axis]);
                    global_max[axis] = global_max[axis].max(p[v + axis]);
                }
            }
        }
        
        // Second pass: extract triangles, group by material, and compute per-material bounds
        let mut bounds_by_material: HashMap<Option<usize>, MaterialBounds> = HashMap::new();
        
        for m in models.iter() {
            let mesh = &m.mesh;
            let material = mesh.material_id;
            
            for n in (0..mesh.indices.len()).step_by(3) {
                let mut idx = (3 * mesh.indices[n]) as usize;
                let v0 = Vector3::new(
                    mesh.positions[idx],
                    mesh.positions[idx + 1],
                    mesh.positions[idx + 2],
                );
                idx = (3 * mesh.indices[n + 1]) as usize;
                let v1 = Vector3::new(
                    mesh.positions[idx],
                    mesh.positions[idx + 1],
                    mesh.positions[idx + 2],
                );
                idx = (3 * mesh.indices[n + 2]) as usize;
                let v2 = Vector3::new(
                    mesh.positions[idx],
                    mesh.positions[idx + 1],
                    mesh.positions[idx + 2],
                );
                
                // Update per-material bounds
                let bounds = bounds_by_material.entry(material).or_insert(MaterialBounds {
                    min: v0,
                    max: v0,
                });
                for v in [v0, v1, v2] {
                    bounds.min.x = bounds.min.x.min(v.x);
                    bounds.min.y = bounds.min.y.min(v.y);
                    bounds.min.z = bounds.min.z.min(v.z);
                    bounds.max.x = bounds.max.x.max(v.x);
                    bounds.max.y = bounds.max.y.max(v.y);
                    bounds.max.z = bounds.max.z.max(v.z);
                }
                
                let uvs = if !mesh.texcoords.is_empty() {
                    idx = (2 * mesh.indices[n]) as usize;
                    let uv0 = Vector2::new(mesh.texcoords[idx], mesh.texcoords[idx + 1]);
                    idx = (2 * mesh.indices[n + 1]) as usize;
                    let uv1 = Vector2::new(mesh.texcoords[idx], mesh.texcoords[idx + 1]);
                    idx = (2 * mesh.indices[n + 2]) as usize;
                    let uv2 = Vector2::new(mesh.texcoords[idx], mesh.texcoords[idx + 1]);
                    Some([uv0, uv1, uv2])
                } else {
                    None
                };
                
                let aabb = AABB::from_triangle(v0, v1, v2);
                let triangle = Triangle {
                    material_id: material,
                    vertices: [v0, v1, v2],
                    uvs,
                    aabb,
                };
                
                by_material.entry(material).or_default().push(triangle);
                total_count += 1;
            }
        }
        
        Self {
            by_material,
            bounds_by_material,
            global_min,
            global_max,
            total_count,
        }
    }
    
    /// Get the bounding box for a specific material.
    pub fn get_material_bounds(&self, material_id: usize) -> Option<&MaterialBounds> {
        self.bounds_by_material.get(&Some(material_id))
    }
    
    /// Get triangles for a specific material ID.
    pub fn get_material(&self, material_id: usize) -> Option<&Vec<Triangle>> {
        self.by_material.get(&Some(material_id))
    }
}

/// Progress tracker for voxelization
pub struct VoxelizeProgress {
    pub triangles_total: AtomicUsize,
    pub triangles_processed: AtomicUsize,
    pub depth_current: AtomicUsize,
    pub depth_max: AtomicUsize,
}

impl VoxelizeProgress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            triangles_total: AtomicUsize::new(0),
            triangles_processed: AtomicUsize::new(0),
            depth_current: AtomicUsize::new(0),
            depth_max: AtomicUsize::new(0),
        })
    }
}

#[allow(dead_code)]
pub fn voxelize(
    models: &[tobj::Model],
    materials: &[RgbaImage],
    _scale: f32,
    _bricktype: BrickType,
    material_filter: Option<usize>,
) -> VoxelTree<Vector4<u8>> {
    voxelize_with_progress(models, materials, _scale, _bricktype, material_filter, None, None)
}

pub fn voxelize_with_progress(
    models: &[tobj::Model],
    materials: &[RgbaImage],
    _scale: f32,
    _bricktype: BrickType,
    material_filter: Option<usize>,
    progress: Option<Arc<VoxelizeProgress>>,
    logger: Option<&Logger>,
) -> VoxelTree<Vector4<u8>> {
    let mut octree = VoxelTree::<Vector4<u8>>::new();

    // Determine model AABB to expand triangle octree to final size
    // Models are already scaled by scale_models()
    let u = &models[0].mesh.positions; // Guess initial
    let mut min = Vector3::new(u[0], u[1], u[2]);
    let mut max = min;

    for m in models.iter() {
        let p = &m.mesh.positions;
        for v in (0..p.len()).step_by(3) {
            for m in 0..3 {
                min[m] = min[m].min(p[v + m]);
                max[m] = max[m].max(p[v + m]);
            }
        }
    }

    let floor_min = Vector3::<isize>::new(
        min[0].floor() as isize,
        min[1].floor() as isize,
        min[2].floor() as isize,
    );
    let ceil_max = Vector3::<isize>::new(
        max[0].ceil() as isize + 1,
        max[1].ceil() as isize + 1,
        max[2].ceil() as isize + 1,
    );

    // Debug: log the model bounds before octree sizing
    if crate::DEBUG_MODE {
        if let Some(log) = logger {
            log.log(format!("[DEBUG] Model float bounds: min({:.2},{:.2},{:.2}) max({:.2},{:.2},{:.2})", 
                min[0], min[1], min[2], max[0], max[1], max[2]));
            log.log(format!("[DEBUG] Integer bounds: floor_min({},{},{}) ceil_max({},{},{})", 
                floor_min[0], floor_min[1], floor_min[2], ceil_max[0], ceil_max[1], ceil_max[2]));
        }
    }

    while !octree.contains_bounds(floor_min) || !octree.contains_bounds(ceil_max) {
        octree.size += 1;
    }

    let mask = 1 << octree.size;
    if crate::DEBUG_MODE {
        if let Some(log) = logger {
            log.log(format!("[DEBUG] Octree size: {}, mask: {}, range: [{}, {}]", 
                octree.size, mask, -(mask as isize), mask as isize));
        }
    }

    // Voxelize
    let mut triangles = Vec::<Triangle>::new();
    for m in models.iter() {
        let mesh = &m.mesh;
        let material = mesh.material_id;

        // Skip if material doesn't match filter
        if let Some(filter_id) = material_filter {
            if material != Some(filter_id) {
                continue;
            }
        }

        for n in (0..mesh.indices.len()).step_by(3) {
            let mut m = (3 * mesh.indices[n]) as usize;
            let v0 = Vector3::new(
                mesh.positions[m],
                mesh.positions[m + 1],
                mesh.positions[m + 2],
            );
            m = (3 * mesh.indices[n + 1]) as usize;
            let v1 = Vector3::new(
                mesh.positions[m],
                mesh.positions[m + 1],
                mesh.positions[m + 2],
            );
            m = (3 * mesh.indices[n + 2]) as usize;
            let v2 = Vector3::new(
                mesh.positions[m],
                mesh.positions[m + 1],
                mesh.positions[m + 2],
            );

            let uvs = if !mesh.texcoords.is_empty() {
                m = (2 * mesh.indices[n]) as usize;
                let uv0 = Vector2::new(mesh.texcoords[m], mesh.texcoords[m + 1]);
                m = (2 * mesh.indices[n + 1]) as usize;
                let uv1 = Vector2::new(mesh.texcoords[m], mesh.texcoords[m + 1]);
                m = (2 * mesh.indices[n + 2]) as usize;
                let uv2 = Vector2::new(mesh.texcoords[m], mesh.texcoords[m + 1]);

                Some([uv0, uv1, uv2])
            } else {
                None
            };

            let aabb = AABB::from_triangle(v0, v1, v2);
            
            // Filter out triangles smaller than a voxel (sub-brick geometry)
            // This reduces memory usage and processing time significantly
            // Threshold: 0.5 voxels - triangles smaller than half a voxel won't contribute meaningfully
            let min_triangle_size = 0.5;
            if aabb.max_dimension() < min_triangle_size {
                continue;
            }
            
            let triangle = Triangle {
                material_id: material,
                vertices: [v0, v1, v2],
                uvs,
                aabb,
            };

            triangles.push(triangle);
        }
    }

    // Set up progress tracking
    if let Some(ref p) = progress {
        p.triangles_total.store(triangles.len(), Ordering::Relaxed);
        p.depth_max.store(octree.size as usize, Ordering::Relaxed);
    }

    recursive_voxelize(&mut octree.contents, mask, triangles, materials, octree.size as usize, &progress);

    octree
}

/// Voxelize from pre-grouped triangles for a specific material.
/// This is much faster when processing multiple materials since triangles are already extracted.
/// Uses per-material bounds with coordinate translation for efficient octree sizing.
/// Returns (octree, world_offset) where world_offset is the translation that was applied.
/// Caller must add world_offset back to brick positions to restore world coordinates.
///
/// ## Design Note
///
/// This function ALWAYS uses per-material bounds for octree sizing. This is critical for
/// performance - a material at Z=500 with size 10x10x10 gets octree size 4 (efficient),
/// not size 10 (which would take 750+ seconds).
///
/// World alignment is handled by the caller adding `world_offset` back to brick positions.
/// This separation of concerns prevents the V3 regression where attempting to fix alignment
/// by changing octree sizing caused processing times to explode from 43 minutes to days.
pub fn voxelize_from_pregrouped(
    pregrouped: &PreGroupedTriangles,
    materials: &[RgbaImage],
    material_id: usize,
    _global_offset: Option<Vector3<f32>>, // DEPRECATED: kept for API compatibility, always ignored
    progress: Option<Arc<VoxelizeProgress>>,
    _logger: Option<&Logger>,
) -> (VoxelTree<Vector4<u8>>, Vector3<f32>) {
    let mut octree = VoxelTree::<Vector4<u8>>::new();
    let zero_offset = Vector3::new(0.0, 0.0, 0.0);
    
    // Get pre-grouped triangles for this material (already extracted)
    let original_triangles = match pregrouped.get_material(material_id) {
        Some(tris) => tris,
        None => return (octree, zero_offset), // No triangles for this material
    };
    
    // Get per-material bounds for efficient octree sizing
    let bounds = match pregrouped.get_material_bounds(material_id) {
        Some(b) => b,
        None => return (octree, zero_offset),
    };
    
    // ALWAYS use per-material bounds.min for translation
    // This keeps octrees small and efficient (size 3-6 instead of 9-10)
    let translation_offset = bounds.min;
    let world_offset = bounds.min;
    
    // Translate triangles to local coordinates (near origin)
    let triangles: Vec<Triangle> = original_triangles.iter().map(|t| {
        let mut translated = *t;
        translated.vertices[0] -= translation_offset;
        translated.vertices[1] -= translation_offset;
        translated.vertices[2] -= translation_offset;
        translated.aabb = AABB::from_triangle(
            translated.vertices[0],
            translated.vertices[1],
            translated.vertices[2],
        );
        translated
    }).collect();
    
    // Calculate bounds for translated triangles
    // Bounds start near origin (0,0,0) for efficient octree sizing
    let size = bounds.max - bounds.min;
    let floor_min = Vector3::<isize>::new(0, 0, 0);
    let ceil_max = Vector3::<isize>::new(
        size[0].ceil() as isize + 1,
        size[1].ceil() as isize + 1,
        size[2].ceil() as isize + 1,
    );
    
    while !octree.contains_bounds(floor_min) || !octree.contains_bounds(ceil_max) {
        octree.size += 1;
    }

    let mask = 1 << octree.size;

    // Set up progress tracking
    if let Some(ref p) = progress {
        p.triangles_total.store(triangles.len(), Ordering::Relaxed);
        p.depth_max.store(octree.size as usize, Ordering::Relaxed);
    }

    recursive_voxelize(&mut octree.contents, mask, triangles, materials, octree.size as usize, &progress);

    (octree, world_offset)
}

/// Minimum depth at which to use parallel processing.
/// Below this depth, the overhead of spawning tasks outweighs the benefits.
const PARALLEL_DEPTH_THRESHOLD: usize = 3;

fn recursive_voxelize(
    branches: &mut Branches<Vector4<u8>>,
    mask: isize,
    vector: Vec<Triangle>,
    materials: &[RgbaImage],
    depth: usize,
    progress: &Option<Arc<VoxelizeProgress>>,
) {
    // Update progress depth
    if let Some(ref p) = progress {
        p.depth_current.store(depth, Ordering::Relaxed);
    }
    let m = mask >> 1;
    let half_box = (2 * m + ((m == 0) as isize)) as f32 / 2.;

    // Use parallel processing for higher depths where there's enough work
    if depth >= PARALLEL_DEPTH_THRESHOLD && m != 0 {
        // Compute results in parallel, then assign back
        let results: Vec<(usize, TreeBody<Vector4<u8>>)> = (0..8usize)
            .into_par_iter()
            .filter_map(|i| {
                // Check if this branch is empty (we only process empty branches)
                let center = Vector3::<f32>::new(
                    half_box * (2 * ((i & 4) > 0) as isize - 1) as f32,
                    half_box * (2 * ((i & 2) > 0) as isize - 1) as f32,
                    half_box * (2 * ((i & 1) > 0) as isize - 1) as f32,
                );

                let mut triangles = Vec::<Triangle>::new();

                for triangle in &vector {
                    // Fast AABB rejection test first
                    if !triangle.aabb.intersects_cube(center, half_box) {
                        continue;
                    }
                    // Full SAT intersection test
                    if intersect(
                        half_box,
                        center,
                        triangle.vertices[0],
                        triangle.vertices[1],
                        triangle.vertices[2],
                    ).is_some() {
                        let mut cloned_triangle = *triangle;
                        cloned_triangle.vertices[0] -= center;
                        cloned_triangle.vertices[1] -= center;
                        cloned_triangle.vertices[2] -= center;
                        cloned_triangle.aabb = triangle.aabb.translated(center);
                        triangles.push(cloned_triangle);
                    }
                }

                if triangles.is_empty() {
                    return None;
                }

                // Recursively build this branch
                let mut sub_branches = TreeBody::empty();
                recursive_voxelize(&mut sub_branches, m, triangles, materials, depth.saturating_sub(1), progress);
                Some((i, TreeBody::Branch(Box::new(sub_branches))))
            })
            .collect();

        // Assign results back to branches
        for (i, body) in results {
            branches[i] = body;
        }
    } else {
        // Sequential processing for lower depths or leaf level
        for (i, branch) in branches.iter_mut().enumerate() {
            if let TreeBody::Empty = branch {
                let center = Vector3::<f32>::new(
                    half_box * (2 * ((i & 4) > 0) as isize - 1) as f32,
                    half_box * (2 * ((i & 2) > 0) as isize - 1) as f32,
                    half_box * (2 * ((i & 1) > 0) as isize - 1) as f32,
                );

                let mut triangles = Vec::<Triangle>::new();
                let mut colors = Vec::<Vector4<u8>>::new();

                for triangle in &vector {
                    // Fast AABB rejection test first
                    if !triangle.aabb.intersects_cube(center, half_box) {
                        continue;
                    }
                    // Full SAT intersection test
                    match intersect(
                        half_box,
                        center,
                        triangle.vertices[0],
                        triangle.vertices[1],
                        triangle.vertices[2],
                    ) {
                        Some(intersection) => {
                            // Only calculate colors if in root level
                            if m == 0 {
                                if let Some(id) = triangle.material_id {
                                    let uv =
                                        interpolate_uv(&triangle.vertices, &triangle.uvs, intersection);
                                    let mat = &materials[id];

                                    let u = ((uv[0] - uv[0].floor()) * (mat.width() - 1) as f32) as u32;
                                    let v =
                                        ((1. - uv[1] + uv[1].floor()) * (mat.height() - 1) as f32) as u32;

                                    let c = *mat.get_pixel(u, v);
                                    if c[3] == 0 {
                                        continue;
                                    } // If alpha is zero, skeedaddle
                                    colors.push(Vector4::<u8>::new(c[0], c[1], c[2], c[3]));
                                }
                            }
                        }
                        None => continue,
                    }

                    let mut cloned_triangle = *triangle;
                    cloned_triangle.vertices[0] -= center;
                    cloned_triangle.vertices[1] -= center;
                    cloned_triangle.vertices[2] -= center;
                    cloned_triangle.aabb = triangle.aabb.translated(center);

                    triangles.push(cloned_triangle);
                }

                if triangles.is_empty() {
                    continue;
                }
                if m != 0 {
                    // Not yet at root level, keep on recursing...
                    *branch = TreeBody::Branch(Box::new(TreeBody::empty()));
                    if let TreeBody::Branch(b) = branch {
                        recursive_voxelize(b, m, triangles, materials, depth.saturating_sub(1), progress);
                    }
                } else {
                    *branch = TreeBody::Leaf(hsv2rgb(hsv_average(&colors)));
                    // Update progress when we complete a leaf (actual voxel)
                    if let Some(ref p) = progress {
                        p.triangles_processed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}
