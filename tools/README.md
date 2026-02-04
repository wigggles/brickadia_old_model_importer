# Log Analysis Tools

## analyze_log.py

A Python script to analyze obj2brz conversion logs and extract detailed statistics.

### Usage

```bash
python tools/analyze_log.py <log_file> [--csv <output.csv>]
```

### Examples

```bash
# Analyze a log and print summary to console
python tools/analyze_log.py data/user_cache/logs/obj2brz_2026-02-03_14-02-59.log

# Analyze and export detailed statistics to CSV
python tools/analyze_log.py my_log.log --csv stats.csv
```

### Output

The tool provides:

1. **Conversion Settings**: Scale, brick scale, simplify mode, split-by-material mode
2. **Overall Statistics**: Material counts, voxel/brick totals, processing time
3. **Octree Size Distribution**: How many materials use each octree size
4. **Problem Materials**:
   - Materials with 0 voxels extracted
   - Materials with large octrees (size >= 8)
   - Slow materials (>10s processing time)
   - Materials with offset(0.0,0.0,0.0) (indicates voxelization bug)

### CSV Export

The CSV export includes per-material details:
- Material ID and name
- Triangle count
- Octree size
- Voxelization offset (X, Y, Z)
- Voxels extracted
- Bricks generated
- Processing time
- Bounding box (min, max, size)
- Skip status and reason

### Use Cases

1. **Performance Analysis**: Identify slow materials and large octrees
2. **Bug Detection**: Find materials with 0 voxels or invalid offsets
3. **Optimization Tracking**: Compare logs before/after code changes
4. **Material Investigation**: Export CSV for detailed analysis in spreadsheet tools

### Requirements

- Python 3.6+
- No external dependencies (uses only standard library)
