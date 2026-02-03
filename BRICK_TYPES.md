# Brickadia Brick Types Reference

This document indexes all Brickadia brick types from the `brdb` crate, organized by category with size, slope, and direction information to help developers understand what goes into `.brz` save files.

## Overview

Brickadia uses two main brick type prefixes:
- **`B_`** - Fixed-size bricks with specific shapes (non-procedural)
- **`PB_`** - Procedural bricks that can be resized dynamically

The `brdb` crate (version 0.5.0) provides these as constants in `brdb::assets::bricks`.

---

## Procedural Bricks (PB_)

Procedural bricks can be scaled to any valid size using `BrickSize::new(x, y, z)`.

### Basic Shapes

| Constant | Description | Slope | Notes |
|----------|-------------|-------|-------|
| `PB_DEFAULT_BRICK` | Standard studded brick | No | Most common brick type |
| `PB_DEFAULT_MICRO_BRICK` | Micro-scale studded brick | No | Smallest unit (2x2x2 studs) |
| `PB_DEFAULT_TILE` | Flat tile with studs on top | No | Plate-height brick |
| `PB_DEFAULT_SMOOTH_TILE` | Flat tile without studs | No | Smooth surface finish |
| `PB_DEFAULT_STUDDED` | Stud cube (studs on all sides) | No | Decorative studded surface |
| `PB_DEFAULT_POLE` | Cylindrical pole | No | Round cross-section |

### Ramps (Sloped)

| Constant | Description | Slope Direction | Notes |
|----------|-------------|-----------------|-------|
| `PB_DEFAULT_RAMP` | Basic ramp | Up along +Y | Standard slope |
| `PB_DEFAULT_RAMP_INVERTED` | Inverted ramp | Down along +Y | Upside-down slope |
| `PB_DEFAULT_RAMP_CORNER` | Outer corner ramp | Diagonal | 90-degree outer turn |
| `PB_DEFAULT_RAMP_CORNER_INVERTED` | Inverted outer corner | Diagonal | Upside-down outer corner |
| `PB_DEFAULT_RAMP_INNER_CORNER` | Inner corner ramp | Diagonal | 90-degree inner turn |
| `PB_DEFAULT_RAMP_INNER_CORNER_INVERTED` | Inverted inner corner | Diagonal | Upside-down inner corner |
| `PB_DEFAULT_RAMP_CREST` | Ramp crest/peak | Both sides | Top of a roof ridge |
| `PB_DEFAULT_RAMP_CREST_CORNER` | Crest corner | Diagonal | Corner of roof ridge |
| `PB_DEFAULT_RAMP_CREST_END` | Crest end cap | One side | End of roof ridge |

### Wedges (Micro-scale Slopes)

| Constant | Description | Slope Direction | Notes |
|----------|-------------|-----------------|-------|
| `PB_DEFAULT_WEDGE` | Standard wedge | Up along +Y | Full-height wedge |
| `PB_DEFAULT_MICRO_WEDGE` | Micro wedge | Up along +Y | Micro-scale slope |
| `PB_DEFAULT_MICRO_WEDGE_CORNER` | Micro wedge corner | Diagonal | Outer corner wedge |
| `PB_DEFAULT_MICRO_WEDGE_INNER_CORNER` | Micro inner corner | Diagonal | Inner corner wedge |
| `PB_DEFAULT_MICRO_WEDGE_OUTER_CORNER` | Micro outer corner | Diagonal | Outer corner wedge |
| `PB_DEFAULT_MICRO_WEDGE_TRIANGLE_CORNER` | Triangle corner | Diagonal | Triangular corner piece |
| `PB_DEFAULT_MICRO_WEDGE_HALF_INNER_CORNER` | Half inner corner | Diagonal | Half-height inner |
| `PB_DEFAULT_MICRO_WEDGE_HALF_INNER_CORNER_INVERTED` | Half inner inverted | Diagonal | Inverted half inner |
| `PB_DEFAULT_MICRO_WEDGE_HALF_OUTER_CORNER` | Half outer corner | Diagonal | Half-height outer |
| `PB_DEFAULT_MICRO_RAMP` | Micro ramp | Up along +Y | Micro-scale ramp |
| `PB_DEFAULT_SIDE_WEDGE` | Side wedge | Horizontal | Wedge on side face |

### Arches

| Constant | Description | Slope | Notes |
|----------|-------------|-------|-------|
| `PB_DEFAULT_ARCH` | Curved arch | Curved | Architectural arch |

### Mechanical/Joints

| Constant | Description | Notes |
|----------|-------------|-------|
| `PB_SLIDER_JOINT` | Slider joint | Linear motion |
| `PB_MOTOR_SLIDER_JOINT` | Motorized slider | Powered linear motion |
| `PB_SERVO_SLIDER_JOINT` | Servo slider | Precise linear motion |

### Decorative

| Constant | Description | Notes |
|----------|-------------|-------|
| `PB_BAGUETTE` | Baguette bread | Decorative food item |
| `PB_PICKET_FENCE` | Picket fence | Fence segment |
| `PB_ROUNDED_CAP` | Rounded cap | Dome/cap piece |
| `PB_SPIKE` | Spike | Pointed decorative |

---

## Fixed-Size Bricks (B_)

These bricks have fixed dimensions and cannot be resized.

### Round/Cylindrical Shapes

| Constant | Size | Description |
|----------|------|-------------|
| `B_1X1F_ROUND` | 1x1 flat | Round flat plate |
| `B_1X1_ROUND` | 1x1 | Round brick |
| `B_2X2F_ROUND` | 2x2 flat | Round flat plate |
| `B_2X2_ROUND` | 2x2 | Round brick |
| `B_4X4_ROUND` | 4x4 | Large round brick |
| `B_1X1_CONE` | 1x1 | Cone shape |
| `B_2X2_CONE` | 2x2 | Large cone |
| `B_INVERTED_CONE` | - | Upside-down cone |

### Octagonal Shapes

| Constant | Size | Description |
|----------|------|-------------|
| `B_1X1F_OCTO` | 1x1 flat | Octagonal flat |
| `B_2X2F_OCTO` | 2x2 flat | Octagonal flat |
| `B_1X_OCTO` | 1x | Octagonal brick |
| `B_2X_OCTO` | 2x | Large octagonal |
| `B_1X_OCTO_90DEG` | 1x | 90-degree octo turn |
| `B_1X_OCTO_90DEG_INV` | 1x | Inverted 90-degree |
| `B_2X_OCTO_90DEG` | 2x | Large 90-degree turn |
| `B_2X_OCTO_90DEG_INV` | 2x | Large inverted 90-degree |
| `B_1X_OCTO_T` | 1x | T-junction octo |
| `B_1X_OCTO_T_INV` | 1x | Inverted T-junction |
| `B_2X_OCTO_T` | 2x | Large T-junction |
| `B_2X_OCTO_T_INV` | 2x | Large inverted T |
| `B_2X_OCTO_CONE` | 2x | Octagonal cone |
| `B_2X2F_OCTO_CONVERTER` | 2x2 flat | Square to octo converter |
| `B_2X2F_OCTO_CONVERTER_INV` | 2x2 flat | Inverted converter |

### Corner/Structural

| Constant | Size | Description |
|----------|------|-------------|
| `B_2X2_CORNER` | 2x2 | Corner brick |
| `B_1X2_OVERHANG` | 1x2 | Overhang piece |
| `B_2X2_OVERHANG` | 2x2 | Large overhang |
| `B_2X4_DOOR_FRAME` | 2x4 | Door frame |
| `B_8X8_LATTICE_PLATE` | 8x8 | Lattice plate |

### Tile Corners

| Constant | Size | Description |
|----------|------|-------------|
| `B_1X1F_TILE_CORNER` | 1x1 flat | Tile corner piece |
| `B_1X1F_INVERSE_TILE_CORNER` | 1x1 flat | Inverted tile corner |

### Plate Centers

| Constant | Size | Description |
|----------|------|-------------|
| `B_1X2F_PLATE_CENTER` | 1x2 flat | Centered plate |
| `B_1X2F_PLATE_CENTER_INV` | 1x2 flat | Inverted center plate |
| `B_2X2F_PLATE_CENTER` | 2x2 flat | Large centered plate |
| `B_2X2F_PLATE_CENTER_INV` | 2x2 flat | Large inverted center |

### Side Bricks

| Constant | Size | Description |
|----------|------|-------------|
| `B_1X1_BRICK_SIDE` | 1x1 | Side-facing brick |
| `B_1X1_BRICK_SIDE_LIP` | 1x1 | Side brick with lip |
| `B_1X4_BRICK_SIDE` | 1x4 | Long side brick |

### Slipper/Slope Bricks

| Constant | Size | Description |
|----------|------|-------------|
| `B_2X1_SLIPPER` | 2x1 | Slipper slope |
| `B_2X2_SLIPPER` | 2x2 | Large slipper |

### Decorative/Props

| Constant | Description |
|----------|-------------|
| `B_BONE` | Bone prop |
| `B_BONE_STRAIGHT` | Straight bone |
| `B_BRANCH` | Tree branch |
| `B_CAULDRON` | Cauldron pot |
| `B_CHALICE` | Chalice cup |
| `B_COFFIN` | Coffin |
| `B_COFFIN_LID` | Coffin lid |
| `B_FERN` | Fern plant |
| `B_FLAME` | Flame effect |
| `B_FLOWER` | Flower |
| `B_SMALL_FLOWER` | Small flower |
| `B_FORK` | Fork utensil |
| `B_SPOON` | Spoon utensil |
| `B_FROG` | Frog |
| `B_FROG_SMALL` | Small frog |
| `B_GRAVESTONE` | Gravestone |
| `B_HANDLE` | Handle |
| `B_JAR` | Jar container |
| `B_LADDER` | Ladder |
| `B_LEAF_BUSH` | Leaf bush |
| `B_PINE_TREE` | Pine tree |
| `B_PUMPKIN` | Pumpkin |
| `B_PUMPKIN_CARVED` | Carved pumpkin |
| `B_SAUSAGE` | Sausage food |
| `B_TURKEY_BODY` | Turkey body |
| `B_TURKEY_LEG` | Turkey leg |
| `B_1X2_METAL_INGOT` | Metal ingot |
| `B_SWIRL_PLATE` | Swirl plate |

### Hedge/Fence

| Constant | Size | Description |
|----------|------|-------------|
| `B_HEDGE_1X1` | 1x1 | Small hedge |
| `B_HEDGE_1X2` | 1x2 | Medium hedge |
| `B_HEDGE_1X4` | 1x4 | Long hedge |
| `B_HEDGE_1X1_CORNER` | 1x1 | Hedge corner |

### Chess Pieces

| Constant | Description |
|----------|-------------|
| `B_PAWN` | Chess pawn |
| `B_ROOK` | Chess rook |
| `B_KNIGHT` | Chess knight |
| `B_BISHOP` | Chess bishop |
| `B_QUEEN` | Chess queen |
| `B_KING` | Chess king |

### Audio/Interaction

| Constant | Description |
|----------|-------------|
| `B_1X1F_SPEAKER` | Small speaker |
| `B_2X2F_SPEAKER` | Large speaker |
| `B_1X1_SOUND_EMITTER` | Sound emitter |
| `B_BUTTON` | Round button |
| `B_BUTTON_SQUARE` | Square button |
| `B_2X2F_TARGET` | Target plate |

### Joints/Mechanical

| Constant | Description |
|----------|-------------|
| `B_JOINT_BEARING` | Bearing joint |
| `B_JOINT_BEARING_MICRO` | Micro bearing |
| `B_JOINT_COUPLER` | Coupler joint |
| `B_JOINT_MOTOR` | Motor joint |
| `B_JOINT_MOTOR_MICRO` | Micro motor |
| `B_JOINT_SERVO` | Servo joint |
| `B_JOINT_SERVO_MICRO` | Micro servo |
| `B_JOINT_SOCKET_MICRO` | Micro socket |
| `B_JOINT_WHEEL` | Wheel joint |
| `B_JOINT_WHEEL_MICRO` | Micro wheel |
| `B_VEHICLE_ENGINE` | Vehicle engine |
| `B_SEAT` | Vehicle seat |

### Game/Spawn Points

| Constant | Description |
|----------|-------------|
| `B_SPAWN_POINT` | Player spawn |
| `B_BOT_SPAWN_POINT` | Bot spawn |
| `B_CHECK_POINT` | Checkpoint |
| `B_GOAL_POINT` | Goal point |
| `B_DESTINATION_POINT` | Destination |

### Logic Gates

| Constant | Description |
|----------|-------------|
| `B_GATE_ADD` | Addition gate |
| `B_GATE_SUBTRACT` | Subtraction gate |
| `B_GATE_MULTIPLY` | Multiplication gate |
| `B_GATE_DIVIDE` | Division gate |
| `B_GATE_MOD` | Modulo gate |
| `B_GATE_MOD_FLOORED` | Floored modulo |
| `B_GATE_FLOOR` | Floor gate |
| `B_GATE_CEILING` | Ceiling gate |
| `B_GATE_CONSTANT` | Constant value |
| `B_GATE_BUFFER` | Buffer gate |
| `B_GATE_BUFFER_TICK` | Tick buffer |
| `B_GATE_BLEND` | Blend gate |
| `B_GATE_EDGE_DETECTOR` | Edge detector |
| `B_REROUTE` | Wire reroute |

### Boolean Logic Gates

| Constant | Description |
|----------|-------------|
| `B_GATE_BOOL_AND` | Boolean AND |
| `B_GATE_BOOL_OR` | Boolean OR |
| `B_GATE_BOOL_NOT` | Boolean NOT |
| `B_GATE_BOOL_NAND` | Boolean NAND |
| `B_GATE_BOOL_NOR` | Boolean NOR |
| `B_GATE_BOOL_XOR` | Boolean XOR |

### Bitwise Logic Gates

| Constant | Description |
|----------|-------------|
| `B_GATE_BIT_AND` | Bitwise AND |
| `B_GATE_BIT_OR` | Bitwise OR |
| `B_GATE_BIT_NOT` | Bitwise NOT |
| `B_GATE_BIT_NAND` | Bitwise NAND |
| `B_GATE_BIT_NOR` | Bitwise NOR |
| `B_GATE_BIT_XOR` | Bitwise XOR |
| `B_GATE_BIT_SHIFT_LEFT` | Left shift |
| `B_GATE_BIT_SHIFT_RIGHT` | Right shift |

### Comparison Gates

| Constant | Description |
|----------|-------------|
| `B_GATE_EQUAL` | Equality check |
| `B_GATE_NOT_EQUAL` | Inequality check |
| `B_GATE_LESS_THAN` | Less than |
| `B_GATE_LESS_THAN_EQUAL` | Less than or equal |
| `B_GATE_GREATER_THAN` | Greater than |
| `B_GATE_GREATER_THAN_EQUAL` | Greater than or equal |

### Entity/Teleport Gates

| Constant | Description |
|----------|-------------|
| `B_1X1_GATE_TELEPORT` | Teleport gate |
| `B_1X1_GATE_RELATIVE_TELEPORT` | Relative teleport |
| `B_1X1_ENTITY_GATE_SET_LOCATION` | Set entity location |
| `B_1X1_ENTITY_GATE_SET_LOCATION_AND_ROTATION` | Set location + rotation |
| `B_1X1_ENTITY_GATE_ADD_LOCATION_AND_ROTATION` | Add to location + rotation |
| `B_1X1_ENTITY_GATE_SET_VELOCITY` | Set entity velocity |
| `B_1X1_ENTITY_GATE_ADD_VELOCITY` | Add to velocity |
| `B_1X1_ENTITY_GATE_PLAY_AUDIO_AT` | Play audio at location |
| `B_1X1_ENTITY_GATE_READ_BRICK_GRID` | Read brick grid |
| `B_1X1_CHARACTER_GATE_SET_GRAVITY_DIRECTION` | Set gravity direction |
| `B_1X1_GATE_WHEEL_ENGINE_SLIM` | Slim wheel engine gate |

### Plate Types

| Constant | Description |
|----------|-------------|
| `BP_LATTICE_THIN` | Thin lattice plate |
| `BP_ROUND_PLATE` | Round plate |
| `BP_SPIKE_PLATE` | Spike plate |
| `BP_SQUARE_PLATE` | Square plate |

### Coins

| Constant | Description |
|----------|-------------|
| `B_1X1_COIN` | Coin |
| `B_1X1_COIN_DIAGONAL` | Diagonal coin |

---

## BrickType Enum

The `brdb::BrickType` enum defines how a brick's asset is specified:

```rust
pub enum BrickType {
    // Fixed asset with no size customization
    Fixed { asset: BString },
    
    // Procedural asset with custom size
    Procedural { asset: BString, size: BrickSize },
}
```

### BrickSize

Procedural bricks use `BrickSize` to define dimensions:

```rust
pub struct BrickSize {
    pub x: u16,  // Width in micro-units (2 = 1 stud)
    pub y: u16,  // Depth in micro-units
    pub z: u16,  // Height in micro-units
}

impl BrickSize {
    pub fn new(x: u16, y: u16, z: u16) -> Self;
}
```

**Unit conversion:**
- 1 stud = 2 micro-units (for X and Y)
- 1 plate height = 2 micro-units (for Z)
- 1 brick height = 6 micro-units (3 plates)

---

## Direction and Rotation

Bricks can be rotated using the `Direction` and `Rotation` enums:

```rust
pub enum Direction {
    XPositive,  // +X facing
    XNegative,  // -X facing
    YPositive,  // +Y facing
    YNegative,  // -Y facing
    ZPositive,  // +Z facing (up)
    ZNegative,  // -Z facing (down)
}

pub enum Rotation {
    Deg0,    // 0 degrees
    Deg90,   // 90 degrees
    Deg180,  // 180 degrees
    Deg270,  // 270 degrees
}
```

These are combined into an orientation byte using `orientation_to_byte(direction, rotation)`.

---

## Materials

Bricks can have different materials from `brdb::assets::materials`:

| Constant | Description |
|----------|-------------|
| `PLASTIC` | Standard opaque plastic |
| `GLASS` | Transparent glass |
| `GLOW` | Emissive/glowing material |
| `METALLIC` | Shiny metallic surface |
| `HOLOGRAM` | Translucent holographic |
| `GHOST` | Semi-transparent, no collision |

---

## Usage Example

```rust
use brdb::{Brick, BrickType, BrickSize, Position, Color};
use brdb::assets::bricks::PB_DEFAULT_MICRO_BRICK;
use brdb::assets::materials::PLASTIC;

let brick = Brick {
    asset: BrickType::Procedural {
        asset: PB_DEFAULT_MICRO_BRICK,
        size: BrickSize::new(2, 2, 2),  // 1x1x1 stud micro brick
    },
    position: Position::new(0, 0, 0),
    color: Color { r: 255, g: 0, b: 0 },
    material: PLASTIC,
    material_intensity: 5,
    ..Default::default()
};
```

---

## References

- [brdb crate documentation](https://docs.rs/brdb)
- [brdb::assets::bricks](https://docs.rs/brdb/latest/brdb/assets/bricks/index.html)
- [brdb::assets::materials](https://docs.rs/brdb/latest/brdb/assets/materials/index.html)
- [Brickadia Community GitHub](https://github.com/brickadia-community)
