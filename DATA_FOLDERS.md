# Data Folder Structure

The `obj2brs` application uses a `data/` folder for storing imports, exports, logs, and user cache.

## Folder Structure

```
<project_root>/data/
├── imports/           # Place input files here (OBJ, BSP)
├── exports/           # Converted .brz saves are output here
└── user_cache/        # User-specific cache data (can be flushed)
    └── logs/          # Application logs with timestamps
        └── obj2brs_YYYY-MM-DD_HH-MM-SS.log
```

## Data Location Priority

The application looks for the `data/` folder in this order:

1. **Current working directory** (development mode) - `./data/`
2. **Executable directory** (distributed mode) - `<exe_dir>/data/`
3. **Fallback** - Creates `data/` in current directory

| Run Mode | Data Folder |
|----------|-------------|
| Development (`.\run.ps1`) | `<project_root>/data/` |
| Distributed exe | `<exe_dir>/data/` |

## Development Workflow

### Running from Source

```powershell
.\run.ps1           # Debug mode - fast compile
.\run.ps1 -Release  # Release mode - optimized
```

**Data location:** `<project_root>/data/` (shared for all source builds)

- Imports: `data/imports/`
- Exports: `data/exports/`
- Logs: `data/user_cache/logs/`

### Running Distributed Executable

```powershell
.\dist\obj2brs-windows-x86_64.exe
```

**Data location:** `dist/data/` (you need to create this or copy from project root)

## Important Notes

1. **Shared data folder for development** - When running from source (`.\run.ps1`), all builds use the same `data/` folder at the project root. This makes testing easier.

2. **Data folders are auto-created** - The application automatically creates the folder structure on startup if it doesn't exist.

3. **Default paths in GUI** - When the app starts, it auto-suggests:
   - Input: `data/imports/`
   - Output: `data/exports/`

4. **User can override paths** - The GUI allows selecting any folder for input/output.

5. **Logs are always written** - Every session creates a new timestamped log file in `data/user_cache/logs/`.

## Flushing Cache

The `user_cache/` folder contains logs and temporary data. To clear it:

```powershell
# Delete all cache data
Remove-Item -Recurse -Force data\user_cache\*
```

Or use the application's cache flush feature (if available in GUI).

## Distributing the Application

When distributing the built executable:

1. Copy the executable to a folder (e.g., `dist/`)
2. Create a `data/` folder next to it with `imports/` and `exports/` subfolders
3. Users place their files in `data/imports/` and find output in `data/exports/`

```
dist/
├── obj2brs-windows-x86_64.exe
└── data/
    ├── imports/
    ├── exports/
    └── user_cache/
```
