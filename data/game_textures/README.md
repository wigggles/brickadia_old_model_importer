# Game Textures - Material Mapping Configuration

This directory contains texture-to-material mapping configurations for automatic Brickadia material assignment during BSP conversion.

## Important Legal Notice

**Game textures are copyrighted material and cannot be distributed with this software.**

You must obtain game textures legally by:

1. **Owning a legitimate copy** of the game (purchased from Steam, GOG, retail, etc.)
2. **Extracting textures yourself** from your legally owned game files
3. **Placing extracted textures** in the appropriate `assets/` subfolder

This software does not include, distribute, or provide access to any copyrighted game assets. The `assets/` folders are empty placeholders for your own legally obtained textures.

Distributing copyrighted game textures without permission from the copyright holder is illegal and violates the terms of service of most games.

## Directory Structure

```
game_textures/
├── README.md                    # This file
├── goldsrc/                     # GoldSrc engine (Valve)
│   ├── halflife/                # Half-Life 1
│   │   ├── auto_2block_materials.yaml  # Material mapping config
│   │   └── assets/              # Place extracted game textures here
│   └── cstrike/                 # Counter-Strike 1.6 (create as needed)
├── idtech/                      # id Tech engine (id Software)
│   ├── quake/                   # Quake 1
│   ├── quake2/                  # Quake 2
│   └── mohaa/                   # Medal of Honor: Allied Assault
└── iw/                          # IW engine (Infinity Ward)
    ├── cod1/                    # Call of Duty 1
    └── cod2/                    # Call of Duty 2
```

## How It Works

When you enable **"Auto-assign Materials (Experimental)"** in the UI:

1. The converter detects the game source from the BSP file (e.g., "Half-Life 1 (GoldSrc)")
2. It looks for a `auto_2block_materials.yaml` config in the corresponding game folder
3. Each texture name is matched against the prefixes defined in the config
4. Matching textures get the corresponding Brickadia material (Glass, Metallic, Glow, etc.)

## Requirements

- **Split by Material** must be enabled (each texture becomes a separate brick grid)
- A valid `auto_2block_materials.yaml` config must exist for the detected game
- If no config is found, the option will be disabled in the UI

## YAML Config Format

```yaml
# Default material for unmatched textures
default: Plastic
default_intensity: 5

# Each material category has:
#   - intensity: material intensity 0-10 (optional)
#   - prefixes: list of texture name prefixes to match

glass:
  intensity: 3
  prefixes:
    - GLASS
    - WINDOW

metallic:
  intensity: 5
  prefixes:
    - METAL
    - STEEL

glow:
  intensity: 8
  prefixes:
    - LIGHT
    - SCREEN
```

## Available Materials

| Material   | Description                          |
|------------|--------------------------------------|
| Plastic    | Default solid material               |
| Glass      | Transparent, see-through             |
| Glow       | Emits light                          |
| Metallic   | Reflective metal surface             |
| Hologram   | Holographic effect                   |
| Ghost      | Semi-transparent, ghostly            |

## Texture Matching

- Texture names are matched **case-insensitively** as **prefixes**
- Example: `GLASS` matches `GLASS1`, `GLASS_BLUE`, `glass_window`, etc.
- First match wins - order your prefixes from most specific to least specific

## Adding Support for New Games

1. Create a folder matching the game's engine folder name:
   - `<engine>/<title>` (e.g., `goldsrc/halflife/`, `idtech/quake/`)

2. Create `auto_2block_materials.yaml` with your texture mappings

3. Optionally place extracted game textures in an `assets/` subfolder
   - A default material is applied to unmatched textures

## Tips

- Use texture names from the BSP file (check the conversion log)
- Adjust intensity values to get the desired visual effect:
  - **0-3**: Subtle effect
  - **4-6**: Normal effect
  - **7-10**: Strong effect
