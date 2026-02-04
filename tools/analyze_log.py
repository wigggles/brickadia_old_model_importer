#!/usr/bin/env python3
"""
Log Analyzer for obj2brz Conversion Logs

Extracts statistics and insights from obj2brz conversion logs to help
identify performance issues, voxelization failures, and material problems.
"""

import re
import sys
import csv
from pathlib import Path
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Optional, Dict, List, Tuple

@dataclass
class MaterialStats:
    name: str = ""
    mat_id: Optional[int] = None
    triangles: int = 0
    octree_size: Optional[int] = None
    offset: Optional[Tuple[float, float, float]] = None
    voxels_extracted: Optional[int] = None
    bricks_generated: Optional[int] = None
    processing_time: Optional[float] = None
    bounds_min: Optional[Tuple[float, float, float]] = None
    bounds_max: Optional[Tuple[float, float, float]] = None
    bounds_size: Optional[Tuple[float, float, float]] = None
    skipped: bool = False
    skip_reason: Optional[str] = None

@dataclass
class ConversionStats:
    total_materials: int = 0
    materials_processed: int = 0
    materials_skipped: int = 0
    materials_with_zero_voxels: int = 0
    materials_with_zero_bricks: int = 0
    total_triangles: int = 0
    total_voxels: int = 0
    total_bricks: int = 0
    total_processing_time: float = 0.0
    octree_size_distribution: Dict[int, int] = field(default_factory=lambda: defaultdict(int))
    material_details: List[MaterialStats] = field(default_factory=list)
    conversion_settings: Dict[str, str] = field(default_factory=dict)

def parse_log(log_path: Path) -> ConversionStats:
    stats = ConversionStats()
    current_material: Optional[MaterialStats] = None
    
    with open(log_path, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.strip()
            
            # Parse conversion settings
            if "Scale:" in line and "Brick Scale" not in line:
                match = re.search(r'Scale: ([\d.]+)', line)
                if match:
                    stats.conversion_settings['Scale'] = match.group(1)
            elif "Brick Scale:" in line:
                match = re.search(r'Brick Scale: (\d+)', line)
                if match:
                    stats.conversion_settings['Brick Scale'] = match.group(1)
            elif "Split by Material:" in line:
                match = re.search(r'Split by Material: (\w+)', line)
                if match:
                    stats.conversion_settings['Split by Material'] = match.group(1)
            elif "Simplify:" in line:
                match = re.search(r'Simplify: (\w+)', line)
                if match:
                    stats.conversion_settings['Simplify'] = match.group(1)
            
            # Parse material count
            if "Found" in line and "materials, processing each separately" in line:
                match = re.search(r'Found (\d+) materials', line)
                if match:
                    stats.total_materials = int(match.group(1))
            
            # Parse triangle extraction
            if "Extracted" in line and "triangles into" in line and "material groups" in line:
                match = re.search(r'Extracted (\d+) triangles', line)
                if match:
                    stats.total_triangles = int(match.group(1))
            
            # Parse material bounds
            if "[DEBUG] Material" in line and "bounds: min(" in line:
                match = re.search(r'Material (\d+) \(([^)]+)\) bounds: min\(([\d.]+),([\d.]+),([\d.]+)\) max\(([\d.]+),([\d.]+),([\d.]+)\) size\(([\d.]+)x([\d.]+)x([\d.]+)\)', line)
                if match:
                    mat_id = int(match.group(1))
                    mat_name = match.group(2)
                    
                    current_material = MaterialStats(
                        name=mat_name,
                        mat_id=mat_id,
                        bounds_min=(float(match.group(3)), float(match.group(4)), float(match.group(5))),
                        bounds_max=(float(match.group(6)), float(match.group(7)), float(match.group(8))),
                        bounds_size=(float(match.group(9)), float(match.group(10)), float(match.group(11)))
                    )
            
            # Parse voxelization results
            if "[DEBUG] Material" in line and "voxelized: octree size" in line:
                match = re.search(r'octree size (\d+), offset\(([\d.]+),([\d.]+),([\d.]+)\)', line)
                if match and current_material:
                    size = int(match.group(1))
                    current_material.octree_size = size
                    current_material.offset = (float(match.group(2)), float(match.group(3)), float(match.group(4)))
                    stats.octree_size_distribution[size] += 1
            
            # Parse voxel extraction
            if "[DEBUG] Extracted" in line and "voxels from octree" in line:
                match = re.search(r'Extracted (\d+) voxels', line)
                if match and current_material:
                    count = int(match.group(1))
                    current_material.voxels_extracted = count
                    stats.total_voxels += count
                    if count == 0:
                        stats.materials_with_zero_voxels += 1
            
            # Parse final material result
            if "Material" in line and "->" in line and "bricks" in line and "in" in line:
                match = re.search(r'Material \d+ \([^)]+, (\d+) tris\) -> (\d+) bricks .* in ([\d.]+)s', line)
                if match and current_material:
                    current_material.triangles = int(match.group(1))
                    bricks = int(match.group(2))
                    time = float(match.group(3))
                    
                    current_material.bricks_generated = bricks
                    current_material.processing_time = time
                    stats.total_bricks += bricks
                    stats.total_processing_time += time
                    
                    if bricks == 0:
                        stats.materials_with_zero_bricks += 1
                    
                    if "(skipped)" in line:
                        current_material.skipped = True
                        stats.materials_skipped += 1
                    else:
                        stats.materials_processed += 1
                    
                    stats.material_details.append(current_material)
                    current_material = None
            
            # Parse skip messages
            if "SKIPPED:" in line:
                match = re.search(r'SKIPPED: (.+)$', line)
                if match and current_material:
                    current_material.skip_reason = match.group(1)
                    current_material.skipped = True
                    stats.materials_skipped += 1
                    stats.material_details.append(current_material)
                    current_material = None
    
    return stats

def print_summary(stats: ConversionStats):
    print("\n=== CONVERSION SUMMARY ===\n")
    
    # Settings
    print("Settings:")
    for key, value in stats.conversion_settings.items():
        print(f"  {key}: {value}")
    
    # Overall stats
    print("\nOverall Statistics:")
    print(f"  Total Materials: {stats.total_materials}")
    print(f"  Materials Processed: {stats.materials_processed}")
    print(f"  Materials Skipped: {stats.materials_skipped}")
    print(f"  Materials with 0 Voxels: {stats.materials_with_zero_voxels}")
    print(f"  Materials with 0 Bricks: {stats.materials_with_zero_bricks}")
    print(f"  Total Triangles: {stats.total_triangles}")
    print(f"  Total Voxels: {stats.total_voxels}")
    print(f"  Total Bricks: {stats.total_bricks}")
    print(f"  Total Processing Time: {stats.total_processing_time:.2f}s")
    
    # Octree size distribution
    print("\nOctree Size Distribution:")
    for size in sorted(stats.octree_size_distribution.keys()):
        count = stats.octree_size_distribution[size]
        print(f"  Size {size}: {count} materials")
    
    # Problem materials
    print("\nProblem Materials:")
    
    zero_voxel_mats = [m for m in stats.material_details if m.voxels_extracted == 0]
    print(f"  Zero voxels extracted ({len(zero_voxel_mats)}):")
    for mat in zero_voxel_mats[:10]:  # Show first 10
        print(f"    - {mat.name} (octree size: {mat.octree_size}, offset: {mat.offset})")
    if len(zero_voxel_mats) > 10:
        print(f"    ... and {len(zero_voxel_mats) - 10} more")
    
    large_octree_mats = [m for m in stats.material_details if m.octree_size and m.octree_size >= 8]
    print(f"\n  Large octrees (size >= 8) ({len(large_octree_mats)}):")
    for mat in large_octree_mats[:10]:
        print(f"    - {mat.name} (size: {mat.octree_size}, bounds: {mat.bounds_size}, time: {mat.processing_time:.2f}s)")
    if len(large_octree_mats) > 10:
        print(f"    ... and {len(large_octree_mats) - 10} more")
    
    slow_mats = [m for m in stats.material_details if m.processing_time and m.processing_time > 10.0]
    print(f"\n  Slow materials (>10s) ({len(slow_mats)}):")
    for mat in slow_mats[:10]:
        print(f"    - {mat.name} ({mat.processing_time:.2f}s, octree size: {mat.octree_size})")
    if len(slow_mats) > 10:
        print(f"    ... and {len(slow_mats) - 10} more")
    
    # Offset analysis
    zero_offset_mats = [m for m in stats.material_details if m.offset == (0.0, 0.0, 0.0)]
    if zero_offset_mats:
        print(f"\n  Materials with offset(0.0,0.0,0.0) ({len(zero_offset_mats)}):")
        for mat in zero_offset_mats[:5]:
            print(f"    - {mat.name} (voxels: {mat.voxels_extracted}, bricks: {mat.bricks_generated})")
        if len(zero_offset_mats) > 5:
            print(f"    ... and {len(zero_offset_mats) - 5} more")

def export_summary(stats: ConversionStats, output_path: Path):
    """Export summary to a text file."""
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write("=== CONVERSION SUMMARY ===\n\n")
        
        # Settings
        f.write("Settings:\n")
        for key, value in stats.conversion_settings.items():
            f.write(f"  {key}: {value}\n")
        
        # Overall stats
        f.write("\nOverall Statistics:\n")
        f.write(f"  Total Materials: {stats.total_materials}\n")
        f.write(f"  Materials Processed: {stats.materials_processed}\n")
        f.write(f"  Materials Skipped: {stats.materials_skipped}\n")
        f.write(f"  Materials with 0 Voxels: {stats.materials_with_zero_voxels}\n")
        f.write(f"  Materials with 0 Bricks: {stats.materials_with_zero_bricks}\n")
        f.write(f"  Total Triangles: {stats.total_triangles}\n")
        f.write(f"  Total Voxels: {stats.total_voxels}\n")
        f.write(f"  Total Bricks: {stats.total_bricks}\n")
        f.write(f"  Total Processing Time: {stats.total_processing_time:.2f}s ({stats.total_processing_time/60:.1f}min)\n")
        
        # Octree size distribution
        f.write("\nOctree Size Distribution:\n")
        for size in sorted(stats.octree_size_distribution.keys()):
            count = stats.octree_size_distribution[size]
            f.write(f"  Size {size}: {count} materials\n")
        
        # Problem materials
        f.write("\nProblem Materials:\n")
        
        zero_voxel_mats = [m for m in stats.material_details if m.voxels_extracted == 0]
        f.write(f"  Zero voxels extracted ({len(zero_voxel_mats)}):\n")
        for mat in zero_voxel_mats:
            f.write(f"    - {mat.name} (octree size: {mat.octree_size}, offset: {mat.offset})\n")
        
        large_octree_mats = [m for m in stats.material_details if m.octree_size and m.octree_size >= 8]
        f.write(f"\n  Large octrees (size >= 8) ({len(large_octree_mats)}):\n")
        for mat in large_octree_mats:
            f.write(f"    - {mat.name} (size: {mat.octree_size}, bounds: {mat.bounds_size}, time: {mat.processing_time:.2f}s)\n")
        
        slow_mats = [m for m in stats.material_details if m.processing_time and m.processing_time > 10.0]
        f.write(f"\n  Slow materials (>10s) ({len(slow_mats)}):\n")
        for mat in slow_mats:
            f.write(f"    - {mat.name} ({mat.processing_time:.2f}s, octree size: {mat.octree_size})\n")
        
        # Offset analysis
        zero_offset_mats = [m for m in stats.material_details if m.offset == (0.0, 0.0, 0.0)]
        if zero_offset_mats:
            f.write(f"\n  Materials with offset(0.0,0.0,0.0) ({len(zero_offset_mats)}):\n")
            for mat in zero_offset_mats:
                f.write(f"    - {mat.name} (voxels: {mat.voxels_extracted}, bricks: {mat.bricks_generated})\n")

def export_csv(stats: ConversionStats, output_path: Path):
    with open(output_path, 'w', newline='', encoding='utf-8') as f:
        writer = csv.writer(f)
        writer.writerow([
            'MaterialID', 'Material', 'Triangles', 'OctreeSize', 'OffsetX', 'OffsetY', 'OffsetZ',
            'VoxelsExtracted', 'BricksGenerated', 'ProcessingTime',
            'BoundsMinX', 'BoundsMinY', 'BoundsMinZ',
            'BoundsMaxX', 'BoundsMaxY', 'BoundsMaxZ',
            'BoundsSizeX', 'BoundsSizeY', 'BoundsSizeZ',
            'Skipped', 'SkipReason'
        ])
        
        for mat in stats.material_details:
            writer.writerow([
                mat.mat_id or '',
                mat.name,
                mat.triangles,
                mat.octree_size or '',
                mat.offset[0] if mat.offset else '',
                mat.offset[1] if mat.offset else '',
                mat.offset[2] if mat.offset else '',
                mat.voxels_extracted if mat.voxels_extracted is not None else '',
                mat.bricks_generated if mat.bricks_generated is not None else '',
                f"{mat.processing_time:.2f}" if mat.processing_time else '',
                mat.bounds_min[0] if mat.bounds_min else '',
                mat.bounds_min[1] if mat.bounds_min else '',
                mat.bounds_min[2] if mat.bounds_min else '',
                mat.bounds_max[0] if mat.bounds_max else '',
                mat.bounds_max[1] if mat.bounds_max else '',
                mat.bounds_max[2] if mat.bounds_max else '',
                mat.bounds_size[0] if mat.bounds_size else '',
                mat.bounds_size[1] if mat.bounds_size else '',
                mat.bounds_size[2] if mat.bounds_size else '',
                mat.skipped,
                mat.skip_reason or ''
            ])

def process_log_file(log_path: Path, output_dir: Optional[Path] = None):
    """Process a single log file and export summary + CSV."""
    print(f"\nAnalyzing: {log_path.name}")
    
    stats = parse_log(log_path)
    print_summary(stats)
    
    # Determine output paths
    if output_dir:
        output_dir.mkdir(parents=True, exist_ok=True)
        base_name = log_path.stem
    else:
        output_dir = log_path.parent
        base_name = log_path.stem
    
    # Export summary
    summary_path = output_dir / f"{base_name}_summary.txt"
    export_summary(stats, summary_path)
    print(f"Summary exported to: {summary_path}")
    
    # Export CSV
    csv_path = output_dir / f"{base_name}.csv"
    export_csv(stats, csv_path)
    print(f"CSV exported to: {csv_path}")
    
    return stats

def main():
    if len(sys.argv) < 2:
        print("Usage: python analyze_log.py <log_file_or_directory> [--output-dir <dir>]")
        print("\nAnalyzes obj2brz conversion logs and extracts statistics.")
        print("Automatically exports both summary.txt and CSV files.")
        print("\nOptions:")
        print("  --output-dir <dir>  Directory for output files (default: same as log file)")
        print("  --batch             Process all .log files in the specified directory")
        print("\nExamples:")
        print("  python analyze_log.py my_log.log")
        print("  python analyze_log.py docs/analysis/logs --batch")
        print("  python analyze_log.py my_log.log --output-dir output/")
        sys.exit(1)
    
    input_path = Path(sys.argv[1])
    
    if not input_path.exists():
        print(f"Error: Path not found: {input_path}")
        sys.exit(1)
    
    # Parse output directory option
    output_dir = None
    if '--output-dir' in sys.argv:
        idx = sys.argv.index('--output-dir')
        if idx + 1 < len(sys.argv):
            output_dir = Path(sys.argv[idx + 1])
    
    # Check for batch mode
    batch_mode = '--batch' in sys.argv
    
    try:
        if input_path.is_dir():
            if not batch_mode:
                print("Error: Directory specified but --batch flag not provided")
                print("Use --batch to process all .log files in the directory")
                sys.exit(1)
            
            log_files = sorted(input_path.glob('*.log'))
            if not log_files:
                print(f"No .log files found in {input_path}")
                sys.exit(1)
            
            print(f"Found {len(log_files)} log file(s) to process")
            for log_file in log_files:
                try:
                    process_log_file(log_file, output_dir or input_path)
                except Exception as e:
                    print(f"Error processing {log_file.name}: {e}")
                    continue
            
            print(f"\n=== Batch processing complete: {len(log_files)} files ===")
        else:
            # Single file mode
            process_log_file(input_path, output_dir)
    
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

if __name__ == '__main__':
    main()
