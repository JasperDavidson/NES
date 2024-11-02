use num::{signum, zero};

pub const CPU_SPEED: usize = 1790000; // 1.79 Mhz
pub const STACK_BASE: usize = 0x100;
pub const STACK_SIZE: usize = 255;
const ROM_BEGIN: u16 = 0x8000;

// CPU memory constants
const RAM: u16 = 0x0000;
const RAM_MIRRORS_END: u16 = 0x1FFF;
const NUM_RAM_MIRRORS: u16 = 4;
const PPU_REGISTERS: u16 = 0x2000;
const PPU_REGISTERS_MIRRORS_END: u16 = 0x3FFF;
const NUM_PPU_MIRRORS: u16 = 1024;
const APU_IO_REGISTERS: u16 = 0x4000;
const APU_IO_REGISTERS_END: u16 = 0x4017;

// PPU memory constants
const PATTERN_TABLES_BEGIN: u16 = 0x0000;
const PATTERN_TABLES_END: u16 = 0x1FFF;
const NAME_TABLES_BEGIN: u16 = 0x2000;
const NAME_TABLE_SIZE: u16 = 0x400;
const NAME_TABLES_END: u16 = 0x2FFF;
const PALETTE_RAM_BEGIN: u16 = 0x3F00;
const PALETTE_RAM_END: u16 = 0x3FFF;
const NUM_PALETTE_REGISTERS: usize = 32;

// Screen constants
pub const SCREEN_WIDTH: usize = 256; // Both in pixels
pub const SCREEN_HEIGHT: usize = 240;
const NUM_SCANLINES: usize = 261;
const SPRITE_HEIGHT: usize = 8; // 8 pixels/scanlines

pub const NES_TAG: [u8; 4] = [0x4E, 0x45, 0x53, 0x1A];
const PRG_PAGE_SIZE: usize = 16384;
const CHR_PAGE_SIZE: usize = 8192;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Mirroring {
   VERTICAL,
   HORIZONTAL,
   FOUR_SCREEN,
}

#[derive(Clone)]
pub struct Rom {
   pub prg_rom: Vec<u8>,
   pub chr_rom: Vec<u8>,
   pub mapper: u8,
   pub screen_mirroring: Mirroring,
}

impl Rom {
    pub fn new(raw: &Vec<u8>) -> Result<Rom, String> {
        if &raw[0..4] != NES_TAG {
            return Err("File is not in iNES file format".to_string()) // Pretty obvious...
        }

        // Sets various initial states based on the control bytes
        let mapper = (raw[7] & 0b1111_0000) | raw[6];
        let four_screen = (raw[6] & 0b1000) != 0;
        let screen_mirroring = (raw[6] & 0x1) != 0;

        let mirroring = match(four_screen, screen_mirroring) {
            (true, _) => Mirroring::FOUR_SCREEN,
            (false, true) => Mirroring::VERTICAL,
            (false, false) => Mirroring::HORIZONTAL,
        };

        // Sets the program and character ROM size from the control bytes
        let prg_rom_size = raw[4] as usize * PRG_PAGE_SIZE;
        let chr_rom_size = raw[5] as usize * CHR_PAGE_SIZE;

        // Decides whether the trainer should be skipped from the control bytes
        let skip_trainer = (raw[6] & 0b100) != 0;

        // Finds where the program and character ROM starts in the cartridge ROM passed in
        let prg_rom_start = 16 + if skip_trainer { 512 } else { 0 };
        let chr_rom_start = prg_rom_start + prg_rom_size;

        Ok(Rom {
            prg_rom: raw[prg_rom_start..(prg_rom_start + prg_rom_size)].to_vec(),
            chr_rom: raw[chr_rom_start..(chr_rom_start + chr_rom_size)].to_vec(),
            mapper: mapper,
            screen_mirroring: mirroring,
        })
    }
}

pub trait Mem {
    fn mem_read(&mut self, addr: u16) -> u8;
    fn mem_write(&mut self, addr: u16, data: u8);
    fn mem_read_u16(&self, addr: u16) -> u16;
    fn mem_write_u16(&mut self, pos: u16, data: u16);
}

pub struct CPUBus {
    cpu_ram: [u8; (0xFFFF + 1) as usize],
    prg_rom: Vec<u8>, // Program ROM from the cartridge
    halt_flag: bool, // The CPU halts when DMA units are in use
    pub ppu: PPU, // Connecting the PPU to the CPU Bus
    open_bus: u16, // Handles the open-bus case if attempting to access an area with no memory map
    ppu_latch: u8 // The latch is loaded when a value is read/written to the PPU, extracted when read from a write-only latch
}

pub struct PPUBus {
    chr_rom: Vec<u8>, // Character ROM from the cartridge
    vram: [u8; 2048], // Used to lay out the background
    mirroring: Mirroring, // Determines what kind of nametable mirroring is used (vertical, horizontal, four-screen, etc.)
    palette_mem: [u8; 32], // Holds the background colors (low 16 bytes) and sprite colors (high 16 bytes)
    palette_storage: Vec<u8>, // Holds 512 3 byte values, but we really only access the first 64
}

impl CPUBus {
    pub fn new(prg_rom: Vec<u8>, ppu_connection: PPU) -> Self {
        CPUBus {
            cpu_ram: [0; (0xFFFF + 1) as usize],
            prg_rom: prg_rom,
            halt_flag: false,
            ppu: ppu_connection,
            open_bus: 0,
            ppu_latch: 0
        }
    }

    pub fn read_prg_rom(&self, addr: &u16) -> u8 {
        let mut mirrored_addr = addr - 0x8000;
        
        if self.prg_rom.len() == 0x4000 && mirrored_addr >= 0x4000 {
            mirrored_addr %= 0x4000;
        }

        self.prg_rom[mirrored_addr as usize]
    }
}

impl PPUBus {
    pub fn new(chr_rom: Vec<u8>, mirroring: Mirroring, palette_mem: [u8; 32], palette_storage: Vec<u8>) -> Self {
        PPUBus { 
            chr_rom: chr_rom, 
            vram: [0; 2048], 
            mirroring: mirroring, 
            palette_mem: palette_mem,
            palette_storage
        }
    }
}

impl Mem for CPUBus {
    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let mirrored_addr = addr & 0x07FF; // 0x07FF
                self.cpu_ram[mirrored_addr as usize]
            },

            PPU_REGISTERS..=PPU_REGISTERS_MIRRORS_END => {
                let mirrored_addr = addr & 0x2007;

                match mirrored_addr {
                    // Returns the PPU status register and resets the write latch (w register)
                    0x2002 => {
                        self.ppu.w = 0;
                        self.ppu_latch = self.ppu.status;

                        return self.ppu.status
                    }

                    // Returns a byte from OAM
                    0x2004 => {
                        let return_value = self.ppu.oam[self.ppu.oam_addr as usize];
                        self.ppu_latch = return_value;

                        return return_value
                    }

                    // Sets the vram latch to a byte from VRAM
                    // If the address is from palette memory it is instantly returned
                    0x2007 => {
                        let return_value;
                        if (self.ppu.v <= 0x3FFF) & (self.ppu.v >= 0x3F00) {
                            return_value = self.ppu.read_byte(self.ppu.v);
                            // Still sets the ppu latch to the value "underneath"
                            self.ppu.vram_latch = self.ppu.read_byte(self.ppu.v % 0x2000);
                        } else {
                            return_value = self.ppu.vram_latch;
                            self.ppu.vram_latch = self.ppu.read_byte(self.ppu.v);
                        }

                        self.ppu.increment_vram();

                        return return_value
                    },

                    _ => { self.ppu_latch }
                }
            },

            APU_IO_REGISTERS..=APU_IO_REGISTERS_END => {
                // TODO: APU NOT IMPLEMENTED YET
                return 0
            },

            0x8000..=0xFFFF => { self.read_prg_rom(&addr) },
            
            _ => { self.open_bus as u8 }
        }
    }

    fn mem_read_u16(&self, addr: u16) -> u16 {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let unmirrored_addr = addr & 0x07FF;
                let low_byte = self.cpu_ram[unmirrored_addr as usize];
                let high_byte = self.cpu_ram[(unmirrored_addr.wrapping_add(1)) as usize];

                (high_byte as u16) << 8 | low_byte as u16
            },

            PPU_REGISTERS..=PPU_REGISTERS_MIRRORS_END => {
                return self.ppu_latch as u16
            },

            APU_IO_REGISTERS..=APU_IO_REGISTERS_END => {
                // TODO: APU NOT IMPLEMENTED YET
                return 0
            },

            0x8000..=0xFFFF => { ((self.read_prg_rom(&(addr + 1)) as u16) << 8) | self.read_prg_rom(&addr) as u16 },
            
            _ => { self.open_bus }
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        //println!("write addr: {}", addr);

        match addr {
            RAM..=RAM_MIRRORS_END => {
                let unmirrored_addr = addr & 0x07FF;
                self.cpu_ram[unmirrored_addr as usize] = data;
            },

            PPU_REGISTERS..=PPU_REGISTERS_MIRRORS_END => {
                let mirrored_addr = addr & 0x2007;
                match mirrored_addr {
                    // Sets the control register of the PPU
                    0x2000 => {
                        self.ppu.ctrl = data;
                        self.ppu_latch = data;
                    },

                    // Writes to the ppu mask register
                    0x2001 => {
                        self.ppu.mask = data;
                        self.ppu_latch = data;
                    }

                    // Sets open bus
                    0x2002 => {
                        self.ppu_latch = data;
                    }

                    // Writes an OAM address
                    0x2003 => {
                        self.ppu.oam_addr = data;

                        self.ppu_latch = data;
                    }

                    // Writes to OAM and increments the OAM adder - very dangerous (normally) due to not finishing during VBLANK
                    0x2004 => {
                        self.ppu.oam_data = data;
                        self.ppu.oam_data_set();
                        self.ppu.oam_addr += 1;

                        self.ppu_latch = data;
                    }

                    // Loads the scroll register with the scroll data
                    0x2005 => {
                        self.ppu.load_scroll(data);

                        self.ppu_latch = data;
                    }

                    // Loads the given byte into either the high or low position depending on the w register
                    0x2006 => {
                        self.ppu.load_addr_byte(data as u16);

                        self.ppu_latch = data;
                    },

                    // Sets the value of the data register and moves vram forward
                    0x2007 => {
                        self.ppu.cpu_write_byte(data);

                        self.ppu_latch = data;
                    }

                    // OAM DMA Call... pretty tricky to get right
                    // God damn it this sucks ngl
                    // Mfw I have to refactor my code: _-_
                    0x4014 => {
                        self.ppu.oam_data = data;
                        self.halt_flag = true;

                        self.ppu_latch = data;
                    }

                    _ => { self.ppu_latch = data; }
                }
            },

            APU_IO_REGISTERS..=APU_IO_REGISTERS_END => {
                // TODO: APU NOT IMPLEMENTED YET
            },

            _ => { self.open_bus = data as u16; }
        }
    }

    fn mem_write_u16(&mut self, addr: u16, data: u16) {
        match addr {
            RAM..=RAM_MIRRORS_END => {
                let unmirrored_addr = addr & 0x07FF;
                let low_byte = (data & 0x00FF) as u8;
                let high_byte = (data & 0xFF00) as u8;

                self.cpu_ram[unmirrored_addr as usize] = low_byte;
                self.cpu_ram[(unmirrored_addr + 1) as usize] = high_byte;
            },

            PPU_REGISTERS..=PPU_REGISTERS_MIRRORS_END => {
                let unmirrored_addr = addr & 0x2007;

                match unmirrored_addr {
                    0x2002 => {
                        self.open_bus = data;
                    },

                    _ => {
                        self.open_bus = data;
                    }
                }
            },

            APU_IO_REGISTERS..=APU_IO_REGISTERS_END => {
                // TODO: APU NOT IMPLEMENTED YET
            },

            _ => { self.open_bus = data; }
        }
    }
}

// impl Mem later
impl PPUBus {
    pub fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            PATTERN_TABLES_BEGIN..=PATTERN_TABLES_END => {
                self.chr_rom[addr as usize]
            },

            NAME_TABLES_BEGIN..=NAME_TABLES_END => {
                //println!("The addr is: {}", addr);
                //println!("The mirrored addr is: {}", (addr % 0x400));

                let base_addr: u16 = addr & (NAME_TABLE_SIZE - 1);

                let vram_index: u16 = match (self.mirroring, addr) {
                    (Mirroring::HORIZONTAL, 0x2000..0x2800) |
                    (Mirroring::VERTICAL, 0x2000..0x2400) |
                    (Mirroring::VERTICAL, 0x2800..0x2C00) => base_addr,

                    _ => { base_addr + (NAME_TABLE_SIZE - 1) }
                };

                return self.vram[vram_index as usize]
            },

            PALETTE_RAM_BEGIN..=PALETTE_RAM_END => {
                // This value is what points to the red byte in palette_storage (+1 -> green and +2 -> blue)
                return (addr - PALETTE_RAM_BEGIN) as u8 & (NUM_PALETTE_REGISTERS - 1) as u8
            }

            // Open bus behavior — multiplexed with pins 31-38, accesses the low byte of the address
            _ => { (addr & 0b1111_1111) as u8 }
        }
    }

    pub fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            NAME_TABLES_BEGIN..=NAME_TABLES_END => {
                let base_addr: u16 = addr & (NAME_TABLE_SIZE - 1);

                let vram_index: u16 = match (self.mirroring, addr) {
                    (Mirroring::HORIZONTAL, 0x2000..0x2800) |
                    (Mirroring::VERTICAL, 0x2000..0x2400) |
                    (Mirroring::VERTICAL, 0x2800..0x2C00) => base_addr,
    
                    _ => { base_addr + (NAME_TABLE_SIZE - 1) }
                };

                self.vram[vram_index as usize] = data;
            },

            PALETTE_RAM_BEGIN..=PALETTE_RAM_END => {
                let mirrored_addr = (addr - PALETTE_RAM_BEGIN) & (NUM_PALETTE_REGISTERS - 1) as u16;

                // This value is what points to the red byte in palette_storage (+1 -> green and +2 -> blue)
                self.palette_mem[mirrored_addr as usize] = data;
            }

            _ => { println!("Attempted to write to read-only ppu space at address {:X}!", addr) }
        }
    }
}


// PPU struct to hold registers and the PPUBus
// All of the registers have extensive documentation available on the NESDev wiki... they are far too complicated to fully describe here
pub struct PPU {
    oam: [u8; 256], // Object Attribute Memory, used to store sprites
    secondary_oam: [u8; 32], // Buffer for the sprites currently being rendered on the *next* scanline
    ctrl: u8, // > write - Important flags that control PPU operation
    mask: u8, // > write - Handles different rendering states/color effects
    status: u8, // < read - Reflects the current state of the PPU (useful for timing)
    oam_addr: u8, // > write - CPU writes the address of OAM it wants to access here. Always returns to 0 after a frame
    oam_data: u8, // <> read/write - (Writes increment oam_addr, reads do not). ONLY USE DURING VBLANK else corruption
    scroll: u8, // >> write x2 - Tells PPU which pixel from nametable should be in top left - first is x scroll, second is y scroll
    oam_dma: u8, // > write - Stores the page number of the CPU to start transferring data to OAM
    v: u16, // During rendering, used for the scroll position. Outside of rendering, used as the current VRAM address
    t: u16, // During rendering, specifies the starting coarse-x scroll and startying y scroll. Outside, holds scroll/VRAM addr --> v
    x: u8, // The fine-x position of the current scroll, used during rendering alongside v
    w: u8, // Indicates first or second write for PPUSCROLL or PPUADDR. Clears on reads of PPUSTATUS
    low_pttrn_shift_reg: u16, // A group of shift registers, used to serialize bits for rendering
    high_pttrn_shift_reg: u16,
    low_attr_shift_reg: u8,
    high_attr_shift_reg: u8,
    ppu_latch: u8, // Serves as an address latch — the low 8 bits overlap with the data bus
    vram_latch: u8, // Used to store read values when the PPU reads 0x2007
    nmi: u8,
    color_buffer: [u32; SCREEN_WIDTH * SCREEN_HEIGHT], // Stores the rgb colors of each pixel displayed each frame
    pub state: PpuState, // Keeps the PPU state when alternating between the CPU and PPU
    sprite_y: u8,
    sprite_tile_number: u8,
    sprite_attribute: u8,
    sprite_x: u8,
    back_pixel: u8, // Variable to store the generated background pixel every 8 cycles
    sprite_pixel_buffer: [Sprite; 8*8], // Variable to store the generated sprite pixel from OAM (eight possible eight-pixel sprites)
    sprite_buffer_addr: u8, // Address to store what sprite pixel currently storing at
    pixel: u8, // Stores the current selected pixel between the background and sprite pixel
    window: minifb::Window, // Window to display pixels to. FINALLY!!! Feels so good having made it this far I love this project so much :)
    oam_addr_overflow: bool,
    ppu_bus: PPUBus // Bus to communicate with PPU memory like VRAM and the palette memory
}

pub struct PpuState {
    nametable_data: u8, // These four are just state registers
    attribute_data: u8,
    low_bitplane: u8,
    high_bitplane: u8,
    attribute_latch: u8, // Used to store the current attribute byte being shifted in
    pub scanline: u16, // These two store the current scanline and dot (cycle) data
    pub dots: u16,
    sprite_counter: u8, // Tracks how many sprites have been found for the current scanline
    valid_sprite: bool, // Tracks if the sprite detected is valid and the program should fetch the rest of its data
    secondary_oam_addr: u8, // Tracks the current address in secondary_oam
    sprite_addr: u8, // Address for indexing the bytes after the y address for OAM
    even_odd_frame: bool // Tracks whether the frame is even or odd (if even skip the very first cycle of every frame); true if even
}

#[derive(Clone, Copy)]
pub struct Sprite {
    x_coordinate: u8, // X coordinate of the sprite on the next scanline
    pixel: u8, // Four bit pixel to encode pattern table and attribute table
    priority: u8, // 0 if in front of background; 1 if behind background
    horizontal_flip: u8, // 1 if sprite needs to be flipped horizontally
    vertical_flip: u8, // 1 if sprite needs to be flipped vertically
}

impl PPU {
    // Stand in initialization function — NEEDS TO BE REDONE LATER (probably)
    pub fn init_ppu(chr_rom: Vec<u8>, mirroring: Mirroring, palette_storage: Vec<u8>, window: minifb::Window) -> Self {
        PPU { 
              oam: [0; 256],
              secondary_oam: [0; 32],
              ctrl: 0,
              mask: 0,
              status: 0,
              oam_addr: 0,
              oam_data: 0,
              scroll: 0,
              oam_dma: 0,
              v: 0,
              t: 0,
              x: 0,
              w: 0,
              low_pttrn_shift_reg: 0,
              high_pttrn_shift_reg: 0,
              low_attr_shift_reg: 0,
              high_attr_shift_reg: 0,
              color_buffer: [0; SCREEN_WIDTH * SCREEN_HEIGHT],
              ppu_latch: 0,
              vram_latch: 0,
              nmi: 0,
              state: PpuState { nametable_data: 0, attribute_data: 0, low_bitplane: 0, high_bitplane: 0, attribute_latch: 0, scanline: 0, dots: 0, sprite_counter: 0, valid_sprite: false, secondary_oam_addr: 0, sprite_addr: 0, even_odd_frame: true },
              sprite_y: 0,
              sprite_tile_number: 0,
              sprite_attribute: 0,
              sprite_x: 0,
              back_pixel: 0,
              sprite_pixel_buffer: [Sprite { x_coordinate: 0, pixel: 0, priority: 0, horizontal_flip: 0, vertical_flip: 0 }; 64],
              sprite_buffer_addr: 0,
              pixel: 0,
              window,
              oam_addr_overflow: false,
              ppu_bus: PPUBus::new(chr_rom, mirroring, [0; NUM_PALETTE_REGISTERS], palette_storage) ,
        }
    }

    fn read_byte(&mut self, addr: u16) -> u8 {
        self.ppu_bus.mem_read(addr)
    }

    // Doesn't increase dots because it's through the CPU
    fn cpu_write_byte(&mut self, data: u8) {
        self.ppu_bus.mem_write(self.v, data);
        self.increment_vram();
    }

    // Increments the VRAM address. Used after accessing a VRAM element through the CPU (writing to 0x2007)
    fn increment_vram(&mut self) {
        if self.ctrl & 0x4 > 0 {
            self.v = (self.v + 32) & 0x3FFF; // Going down
        } else {
            self.v = (self.v + 1) & 0x3FFF; // Going across
        }
    }

    // Loads register t with VRAM address bytes through 0x2006 writes
    fn load_addr_byte(&mut self, addr: u16) {
        if self.w == 0 {
            self.t = (self.t & !0b1111_1111_0000_0000) | ((addr << 8) & 0b1111_1111_0000_0000); // Storing the upper byte
            self.w = 1; // Setting the write latch (saying we want to write to the lower byte now)
        } else {
            self.t = (self.t & !0b1111_1111) | (addr & 0b1111_1111); // Storing the lower byte
            self.v = self.t;
            self.w = 0;
        }
    }

    // Sets oam data when called by CPU
    fn oam_data_set(&mut self) {
        self.oam[self.oam_addr as usize] = self.oam_data;
    }

    // Loading t with the scroll data
    fn load_scroll(&mut self, scroll_data: u8) {
        if self.w == 0 {
            self.x = scroll_data & 0b111;
            self.t = (self.t & !0b1_1111) | ((scroll_data as u16) >> 3);
            self.w = 1;
        } else {
            self.t = (self.t & !0b11_1110_0000) | (((scroll_data as u16) << 2) & 0b11_1110_0000);
            self.t = (self.t & !0b111_0000_0000_0000) | (((scroll_data as u16) << 12) & 0b111_0000_0000_0000);
            self.w = 0;
        }
    }

    // Returns the code of the palette table used for the current nametable address
    fn fetch_attribute_data(&mut self, addr: u16) -> u8 {
        let x = (addr % 0x400) % 32;
        let y = (addr % 0x400) / 32;
        let mut attribute_addr = 0x23C0 + (x / 4) + ((y / 4) * 8);

        // Adjusts for mirroring
        if self.ppu_bus.mirroring == Mirroring::HORIZONTAL {
            if self.v > 0x2400 {
                attribute_addr += 0x400 * 2; // Mirroring adjustment for second nametable
            }
        } else if self.ppu_bus.mirroring == Mirroring::VERTICAL {
            if (self.v % 0x800) > 0x400 {
                attribute_addr += 0x400; // Vertical mirroring adjustment
            }
        }

        let palette_value: u8 = self.read_byte(attribute_addr);
        // Determine palette selection based on position within the 4x4 attribute block
        let palette_choice: u8 = match ((x % 4) / 2, (y % 4) / 2) {
            (0, 0) => palette_value & 0x3,  // Top-left quadrant
            (1, 0) => (palette_value >> 2) & 0x3,  // Top-right quadrant
            (0, 1) => (palette_value >> 4) & 0x3,  // Bottom-left quadrant
            (1, 1) => (palette_value >> 6) & 0x3,  // Bottom-right quadrant
            _ => 0, // This shouldn't happen
        };

        return palette_choice
    }

    /// Returns byte present in the pattern table, addr is id from the nametable
    pub fn fetch_pattern_table(&self, high: bool, addr: u8) -> u8 {
        let mut pattern_addr: u16 = 0;

        if self.ctrl & 0b0001_0000 != 0 {
            pattern_addr |= 0b1_0000_0000_0000;
        }

        if high {
            pattern_addr |= 0b1000
        }

        pattern_addr |= (addr as u16) << 4;
        pattern_addr |= (self.v >> 12) & 0b111; // Extracting fine y address

        return self.ppu_bus.mem_read(pattern_addr)
    }

    // Loads the address into the PPU latch (multiplexed with bottom 8 address bits)
    fn load_latch(&mut self, addr: u16) {
        self.ppu_latch = (addr & 0xFF) as u8;
    }

    fn nametable_fetch(&mut self, addr: u16) -> u8 {
        let v = (addr & 0xFF00) | (self.ppu_latch as u16);
        let tile_addr = 0x2000 | (v & 0x0FFF);

        return self.read_byte(tile_addr)
    }

    fn shift(&mut self) {
        self.low_pttrn_shift_reg <<= 1;

        self.high_pttrn_shift_reg <<= 1;

        // Allows for the storage of 16 bits of attribute data!!!!!!!!
        self.low_attr_shift_reg <<= 1;
        self.low_attr_shift_reg |= self.state.attribute_latch & 0b1;

        self.high_attr_shift_reg <<= 1;
        self.high_attr_shift_reg |= self.state.attribute_latch & 0b10;
    }

    fn shift_reload(&mut self) {
        // I don't think this is how reloadng the attribute shift registers will work
        // if self.state.attribute_data & 0b0000_0001 != 0 {
        //     self.low_attr_shift_reg = 0xFF;
        // } else {
        //     self.low_attr_shift_reg = 0;
        // }

        // if self.state.attribute_data & 0b0000_0010 != 0 {
        //     self.high_attr_shift_reg = 0xFF;
        // } else {
        //     self.high_attr_shift_reg = 0;
        // }

        self.state.attribute_latch = self.state.attribute_data;

        self.low_pttrn_shift_reg = self.state.low_bitplane as u16;
        self.high_pttrn_shift_reg = self.state.high_bitplane as u16;
    }

    fn continue_render(&mut self) {
        match self.state.dots % 8 {
            // 1, 3, 5, 7 are all used to store the address in the latch
            1 => {
                self.load_latch(self.v);
            },
            // Fetches the pattern table address from the nametable
            2 => {
                self.state.nametable_data = self.nametable_fetch(self.v);
            },
            3 => {
                self.load_latch(self.v);
            },
            // Fetches the attribute data from the attribute table
            4 => {
                self.state.attribute_data = self.fetch_attribute_data(self.v);
            },
            5 => {
                self.load_latch(self.v);
            },
            // Fetches the low pattern bitplane from the pattern table
            6 => {
                self.state.low_bitplane = self.fetch_pattern_table(false, self.state.nametable_data);
            },
            7 => {
                self.load_latch(self.v);
            },
            // Fetches the high pattern bitplane from the pattern table
            _ => {
                self.state.high_bitplane = self.fetch_pattern_table(true, self.state.nametable_data);

                // Takes care of x scrolling
                // 31 is the horizontal limit of a nametable (5 bits)
                if (self.v & 0b1_1111) == 31 {
                    self.v &= !0b1_1111;
                    self.v ^= 0x0400;
                } else {
                    self.v += 1;
                }
            },
        }

        // Handles y scrolling at the end of the scanline
        if self.state.dots == 256 {
            let mut fine_y = (self.v & 0b111_0000_0000_0000) >> 12;
            
            // Fine y overflows into coarse y
            // Row 29 is the limit vertically, but coarse y can be set out of bounds
            if fine_y < 7 {
                fine_y += 1;
                self.v = (self.v & !0b111_0000_0000_0000) | (fine_y << 12);
            } else {
                // Resets fine y and overflows into coarse y
                self.v &= !0b111_0000_0000_0000;
                let mut coarse_y = (self.v & 0b11_1110_0000) >> 5;

                if coarse_y == 29 {
                    coarse_y = 0;
                    self.v ^= 0x0800;
                } else if coarse_y == 31 {
                    coarse_y = 0;
                } else {
                    coarse_y += 1;
                }

                self.v = (self.v & !0b11_1110_0000) | (coarse_y << 5);
            }

            let coarse_xt = self.t & 0b1_1111;
            self.v = (self.v & !0b1_1111) | coarse_xt;
        }
    }

    // Function to handle sprite evaluation and loading the secondary OAM buffer
    fn sprite_evaluation_tick(&mut self) {
        if self.state.dots == 0 {
            return
        }

        if self.state.dots % 2 == 0 {
            // In the first 64 cycles the secondary OAM buffer is filled with 0xFF regardless
            if self.state.dots < 64 {
                self.secondary_oam[self.state.secondary_oam_addr as usize] = 0xFF;
                self.state.secondary_oam_addr += 1;
                return
            } else if self.state.dots == 64 {
                self.secondary_oam[self.state.secondary_oam_addr as usize] = 0xFF;
                self.state.secondary_oam_addr = 0;
                return
            }

            // Through cycle 256, it then loads the next eight sprites from the scanline into the secondary OAM buffer
            // Ignores the write if there are already full 8 sprites in the buffer
            if self.state.dots <= 256 && self.state.dots > 64 && !self.oam_addr_overflow {
                // Checks if the y-byte received is in a valid range, and if so allow for the next bytes to be fetched
                // Evaluating sprites on the *next* scanline remember
                if self.oam_data as u16 <= self.state.scanline + 1 && self.state.scanline + 1 < (self.oam_data as usize + SPRITE_HEIGHT) as u16 && !self.state.valid_sprite {
                    self.state.valid_sprite = true;
                }

                if self.state.sprite_counter < 8 {
                    if self.state.secondary_oam_addr < 32 {
                        self.secondary_oam[self.state.secondary_oam_addr as usize] = self.oam_data;

                        self.state.secondary_oam_addr += 1;
                    } else {
                        // Reads the value in secondary_oam if it is full
                        let _ = self.secondary_oam[0];
                    }
                } else {
                    // Evaluates once 8 sprites are stored in the secondary oam buffer
                    self.state.sprite_addr = 0;

                    if self.state.valid_sprite {
                        self.status |= 0b0010_0000;
                        self.oam_addr = self.oam_addr.wrapping_add(1);

                        // IMPORTANT: This is a bug that renders the overflow sprite register unstable
                        // Only doing this in order to stay accurate to the original NTSC PPU in the NES
                        // Of course some masochist games actually *exploit* the fact that this bug exists
                        self.state.sprite_addr += 1;
                    }
                }
            } else if self.state.dots < 320 {
                self.state.secondary_oam_addr = 0;

                match (self.state.dots - 1) % 8 {
                    0 => {
                        self.sprite_y = self.secondary_oam[self.state.secondary_oam_addr as usize];
                        self.state.secondary_oam_addr += 1;
                    },
                    1 => {
                        self.state.nametable_data = self.secondary_oam[self.state.secondary_oam_addr as usize];
                        self.state.secondary_oam_addr += 1;
                    },
                    2 => {
                        self.state.attribute_data = self.secondary_oam[self.state.secondary_oam_addr as usize];
                        self.state.secondary_oam_addr += 1;
                    },
                    3 | 4 | 5 | 6 => {
                        self.sprite_x = self.secondary_oam[self.state.secondary_oam_addr as usize];
                    },
                    _ => {
                        self.state.secondary_oam_addr += 1;
                    }
                }
            } else {
                // Clock cycle is wasted trying to reading from secondary oam with byte when it has already gone through them all
            }
        } else {
            if self.oam_addr == 64 {
                self.oam_addr = 0;
                self.oam_addr_overflow = true;
            }

            if self.state.valid_sprite {
                self.state.sprite_addr += 1;
            }

            if self.state.sprite_addr == 4 {
                self.state.valid_sprite = false;
                self.state.sprite_counter += 1;
            }

            self.oam_data = self.oam[((4 * self.oam_addr as usize) + self.state.sprite_addr as usize) as usize];

            if !self.state.valid_sprite {
                self.state.sprite_addr = 0;
                self.oam_addr += 1;
            }
        }
    }

    // Compares the background pixel with all the sprite pixels to see if there is an overlap
    // 0 if sprite pixel is selected; 1 if background pixel is selected
    fn compare_against_sprites(&mut self) {
        for sprite in self.sprite_pixel_buffer {
            if sprite.x_coordinate == self.state.dots as u8 {
                if sprite.pixel & 0b11 != 0 {
                    if sprite.priority == 0 {
                        // Logical and with 0b0001_0000 because a 5th bit of one acceses the sprite palette tables
                        self.pixel = sprite.pixel & 0b0001_0000;
                        return
                    }
                } else if self.back_pixel &0b11 == 0{
                    self.pixel = sprite.pixel;
                    return
                }
            }
        }

        self.pixel = self.back_pixel;
    }

    fn fetch_rgb(&self) -> (u8, u8, u8) {
        let palette_addr = self.ppu_bus.mem_read(PALETTE_RAM_BEGIN as u16 + self.pixel as u16) as usize;

        let r = self.ppu_bus.palette_storage[palette_addr * 3];
        let g = self.ppu_bus.palette_storage[palette_addr * 3 + 1];
        let b = self.ppu_bus.palette_storage[palette_addr * 3 + 2];

        return (r, g, b)
    }

    // Note that the data for the first two tiles should already be fetched from previous scanline
    // Sprites cannot be rendered on the first scanline
    pub fn ppu_tick(&mut self) {
        // println!("Scanline: {}", self.state.scanline);
   
        // Visible scanlines
        if self.state.scanline < 240 {
            // If it's an even frame, the very first cycle is skipped — helps with smoothness when not scrolling
            // Slightly offsets the video signal
            if self.state.even_odd_frame && self.state.dots == 0 && self.state.scanline == 0 {
                // No return statement because we *do* want it to run cycle 1 since cycle 0 is skipped
                self.state.dots += 1;
            } else if self.state.dots == 0 && self.state.scanline == 0 {
                self.state.dots += 1;
                return
            }

            if self.state.dots <= 256 {
                self.continue_render();
                self.sprite_evaluation_tick();
                
                // Every 8 cycles 8 new pixels can be constructed
                if self.state.dots > 8 && (self.state.dots - 1) % 8 == 0 {
                    // Loads the shift registers with values from last 8 cycles
                    self.shift_reload();
                }

                // Assembling a four bit pixel based on the fine x register
                // 0 bit: Low pattern table, 1 bit: High pattern table, 2 bit: Low palette bit, 3 bit: High palette bit
                self.back_pixel = (((self.low_pttrn_shift_reg >> (15 - self.x)) & 0x1) as u8)
                                    | ((((self.high_pttrn_shift_reg >> (15 - self.x)) & 0x1) << 1) as u8)
                                    | (((self.low_attr_shift_reg >> (7 - self.x)) & 0x1) << 2)
                                    | (((self.high_attr_shift_reg >> (7 - self.x)) & 0x1) << 3);

                if self.state.scanline > 0 {
                    self.compare_against_sprites();
                } else {
                    self.pixel = self.back_pixel;
                }

                let (r, g, b) = self.fetch_rgb();
                let rgb_value = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;

                // Stores the color output of the pixel in a buffer
                let index = (self.state.scanline * 256 + self.state.dots) as usize;

                if index < self.color_buffer.len() {
                    self.color_buffer[index] = rgb_value;
                }

                self.shift();
            } else if self.state.dots <= 320 {
                // Garbage nametable fetches — allows for reusing circuitry in hardware
                self.sprite_evaluation_tick();
                self.continue_render();

                if self.state.dots > 8 && self.state.dots % 8 == 1 {
                    self.shift_reload();
                }

                self.sprite_pixel_buffer[self.sprite_buffer_addr as usize] = Sprite{ x_coordinate: self.sprite_x + self.x, 
                    pixel: (((self.low_pttrn_shift_reg >> 15 - self.x) & 0x1) as u8)
                         | ((((self.high_pttrn_shift_reg >> 15 - self.x) & 0x1) << 1) as u8)
                         | (((self.low_attr_shift_reg >> 7 - self.x) & 0x1) << 2)
                         | (((self.high_attr_shift_reg >> 7 - self.x) & 0x1) << 3),
                    priority: (self.state.attribute_data & 0b10_0000 > 0) as u8,
                    horizontal_flip: (self.state.attribute_data & 0b100_0000 > 0) as u8,
                    vertical_flip: (self.state.attribute_data & 0b100_0000 > 0) as u8 };
                
                self.sprite_buffer_addr += 1;
            } else if self.state.dots <= 336 {
                // This will load the pattern shift registers with two tiles worth of data
                // The attribute registers will have their second tile data stored in the self.state.attribute_data latch
                self.continue_render();

                if self.state.dots == 329 {
                    self.shift_reload();
                }

                self.shift();
            } else if self.state.dots <= 341 {
                // Useless clock cycles spent accessing the third tiles nametable byte (only implemented to stay faithful)
                self.read_byte(self.v);
                if self.state.dots == 341 {
                    self.state.dots = 0;
                    self.state.scanline += 1;
                    self.sprite_buffer_addr = 0;

                    return
                }
            }
        }

        // PPU just idles here (NMI is *not* set until scanline 241)
        if self.state.scanline == 240 {
            if self.state.dots == 341 {
                self.state.scanline += 1;
                self.state.dots = 0;

                return
            }
        }

        // Start of Vblank — Generate an NMI if requested by the CPU; also display the next frame
        if self.state.scanline == 241 {
            self.status |= 0b10000000;
            self.oam_addr_overflow = false;

            // Doesn't active the NMI until the *second* PPU cycle
            if self.ctrl & 0b10000000 > 0 && self.state.dots >= 1 {
                self.nmi = 1;
                self.ctrl &= !0b1000_0000;

                self.state.even_odd_frame = !self.state.even_odd_frame;
            }

            if self.state.dots == 341 {
                self.state.scanline += 1;
                self.state.dots = 0;

                // Update the screen
                self.window.update_with_buffer(&self.color_buffer, SCREEN_WIDTH, SCREEN_HEIGHT).expect("Failed to update screen!");
                self.color_buffer.fill(0);

                return
            }
        }

        // End of Vblank (scanline 260) — Cancel the ability to create new NMI's and fill shift registers for next frame (first two tiles)
        // The PPU doesn't do anything during these scanlines — just increases the clock (allows the PPU to change memory during Vblank)
        if self.state.scanline < 261 && self.state.scanline >= 241 {
            // Doesn't active the NMI until the *second* PPU cycle
            // Resets the NMI register in control
            if self.ctrl & 0b10000000 > 0 && self.state.dots >= 1 {
                self.nmi = 1;
                self.ctrl &= !0b1000_0000;
            }

            if self.state.dots == 341{
                self.state.scanline += 1;
                self.state.dots = 0;

                 return
            }
        }

        // Also fetches the first two tiles of the first scanline for the next frame
        if self.state.scanline == 261 {
            // Clearing the 3 flags in PPUSTATUS (0x2002)
            if self.state.dots == 1 {
                self.status &= 0b0001_1111;
            }

            if self.state.dots <= 336 {
                // This will load the pattern shift registers with two tiles worth of data
                // The attribute registers will have their second tile data stored in the self.state.attribute_data latch
                self.continue_render();

                if self.state.dots == 329 {
                    self.shift_reload();
                }

                if self.state.dots < 340 && self.state.even_odd_frame {
                    // Useless clock cycles spent accessing the third tiles nametable byte (only implemented to stay faithful)
                    self.read_byte(self.v);
                }

                self.shift();
            }

            // Reset the dots and scanline for the next frame (also sets the next frame to be even/odd)
            // Also resets various other things
            if self.state.dots == 341 {

                self.state.even_odd_frame = !self.state.even_odd_frame;

                self.state.dots = 0;
                self.state.scanline = 0;

                // Clearning sprite overflow and sprite zero hit
                self.status &= !0x20;
                self.status &= !0x40;

                // Resets the write latch
                self.w = 0;

                return 
            }
        }
        
        self.state.dots += 1;
    }
}

// CPU struct to hold registers and the CPUBus
pub struct CPU {
    cpu_clk: usize, // Clock used to coordinate with the CPU, since they run in parallel (Usually 1 CPU Cycle = 3 PPU Cycles)
    pub accumulator: u8, // Allows the use of the status register for overflow detection, carrying, etc.
    pub x: u8, // Both x and y are used for addressing
    pub y: u8,
    pub pc: u16, // Program counter - allows for 65,536 memory locations
    pub sp: u8, // Allows for indexing into a 256 byte stack,
    pub status: u8, // Status register - 6 bits that each encode different meanings --> NV1B DIZC (Negative, Overflow, Decimal, Interrupt Disable, Zero, Carry) skipping B
    pub cpu_bus: CPUBus, // 64 KiB of memory; stack goes from 0x100 to 0x1FF, descending (top to bottom), and empty (sp points to next available space)
}

impl CPU {
    pub fn init_cpu(prg_rom: Vec<u8>, ppu: PPU) -> Self {
        //println!("{}", prg_rom[0xFFFC % 0x8000] as u16);

        return CPU {
            cpu_clk: 0,
            accumulator: 0,
            x: 0,
            y: 0,
            pc: ((prg_rom[0xFFFD % 0x8000] as u16) << 8) | (prg_rom[0xFFFC % 0x8000] as u16),
            sp: 0xFD,
            status: 0b0010_0100,
            cpu_bus: CPUBus::new(prg_rom, ppu),
        }
    }

    pub fn load_testing_ram(&mut self, initial_state: &Vec<(i64, i64)>) {
        for addr_value_pair in initial_state {
            self.write_byte(addr_value_pair.0 as u16, addr_value_pair.1 as u8);
        }
    }

    // Checks if an NMI has been requested and, if so, acts upon it — Acts just like BRK (which is for IRQ's) but is for NMI's (often Vblank)
    // CPU checks for an NMI every cycle
    fn nmi(&mut self) {
        if self.cpu_bus.ppu.nmi == 1 {
            println!("ENTERED NMI");
            self.cpu_bus.ppu.nmi = 0;

            // padding byte
            let _ = self.fetch_byte();

            let low_pc = (self.pc & 0x00FF) as u8;
            let high_pc = ((self.pc & 0xFF00) >> 8) as u8;

            self.push_stack(high_pc);
            self.push_stack(low_pc);
            self.push_stack(self.status);

            // Fetches the NMI handler address
            let new_low_pc = self.read_byte(0xFFFA);
            let new_high_pc = self.read_byte(0xFFFB);

            self.pc = (new_high_pc as u16) << 8 | new_low_pc as u16;

            self.status |= 0b100;
        }
    }

    fn execute_oam_dma(&mut self, start_addr_high: u8) {
        let start_addr = (start_addr_high as u16) << 8;
        let end_addr = start_addr + 255;

        println!("start addr: {}", start_addr);
        println!("end addr: {}", end_addr);

        for addr in start_addr..=end_addr {
            let sprite_data = self.read_byte(addr);
            self.write_oam(sprite_data);
        }
    }

    // Writes a byte to memory
    pub fn write_byte(&mut self, address: u16, data: u8) {
        // println!("write");
        self.cpu_clk += 1;
        self.cpu_bus.mem_write(address, data);

        for _ in 0..=2 {
            // println!("ppu_tick write");
            self.cpu_bus.ppu.ppu_tick();
        }

        self.nmi();
    }

    // Performs a write to oam data
    pub fn write_oam(&mut self, data: u8) {
        // Does NOT call write_byte() because the CPU is halted so the clock doesn't tick up
        self.cpu_bus.mem_write(0x2004, data);
    }

    pub fn read_byte(&mut self, address: u16) -> u8 {
        // println!("read");
        // If a halt has been requested, the CPU stops and executes the DMA — only allowed on read cycles
        if self.cpu_bus.halt_flag {
            self.cpu_bus.halt_flag = false;
            self.execute_oam_dma(self.cpu_bus.ppu.oam_dma);
        }

        self.cpu_clk += 1;
        let rtrn = self.cpu_bus.mem_read(address);

        for _ in 0..=2 {
            // println!("ppu_tick read");
            self.cpu_bus.ppu.ppu_tick();
        }
        
        self.nmi();

        return rtrn
    }

    fn fetch_byte(&mut self) -> u8 {
        let byte = self.read_byte(self.pc);
        self.pc = self.pc.wrapping_add(1);

        byte
    }

    fn fetch_word(&mut self) -> u16 {
        let word = self.cpu_bus.mem_read_u16(self.pc);
        self.pc = self.pc.wrapping_add(2);

        word
    }

    // Helper functions for DEC and INC instructions
    fn dec(&mut self, addr: u16) {
        let data = self.read_byte(addr).wrapping_sub(1);
        self.write_byte(addr, data);
        
        let at_addr = self.read_byte(addr);
        self.set_zero_neg(at_addr);
    }

    fn inc(&mut self, addr: u16) {
        let data = self.read_byte(addr).wrapping_add(1);
        self.write_byte(addr, data);

        let at_addr = self.read_byte(addr);
        self.set_zero_neg(at_addr);
    }

    // Helper functions to keep code more clean (L borrow checker)
    fn or_and_store(&mut self, addr: u16, or_value: u8) {
        let new_value = self.read_byte(addr) | or_value;
        self.write_byte(addr, new_value);
    }
    
    fn and_and_store(&mut self, addr: u16, and_value: u8) {
        let new_value = self.read_byte(addr) & and_value;
        self.write_byte(addr, new_value);
    }

    fn left_shift_and_store(&mut self, addr: u16, ls_value: u8) {
        let new_value = self.read_byte(addr) << ls_value;
        self.write_byte(addr, new_value);
    }
    
    fn right_shift_and_store(&mut self, addr: u16, rs_value: u8) {
        let new_value = self.read_byte(addr) >> rs_value;
        self.write_byte(addr, new_value);
    }

    fn set_zero_neg_flags(&mut self, addr: u16) {
        let result = self.read_byte(addr);
        self.set_zero_neg(result);
    }

    pub fn decode(&mut self) {
        let instruction = self.fetch_byte();
        println!("instruction: {:X}", instruction);
        // println!("pc: {}, s: {}, a: {}, x: {}, y: {}, p: {}", self.pc.wrapping_sub(1), self.sp, self.accumulator, self.x, self.y, self.status);

        let aaa = (instruction >> 5) & 0b111;
        let bbb = (instruction >> 2) & 0b111;
        let cc = instruction & 0b11;


        // Refactored the code to have a function that checks the operand instead — this should lend itself to being cycle accurate
        // (Because we're no longer unecessary calling addressing modes, which introduced redundant read/write cycles)

        // let mut operand: u8 = match bbb {
        //     0 => { self.indexed_indirect() },
        //     1 => { self.zero_page() },
        //     2 => { self.immediate() },
        //     3 => { self.absolute() },
        //     4 => { self.indirect_indexed() },
        //     5 => { self.zero_page_x() },
        //     6 => { self.absolute_y() },
        //     7 => { self.absolute_x() },
        //     _ => { panic!("Addressing mode not supported") }, 
        // };

        match (aaa, bbb, cc) {
            // Implied Instructions - These do not care about the operand value as it's not relevant

            // BRK - Forces the generation of an interrupt request
            // Loads the program counter and status flags onto the stack then IRQ interrupt vector is loaded into the pc
            // Break status flag is set to 1 to prevent further interrupts

            (0, 0, 0) => {
                // padding byte
                let _ = self.fetch_byte();

                let low_pc = (self.pc & 0x00FF) as u8;
                let high_pc = ((self.pc & 0xFF00) >> 8) as u8;

                self.push_stack(high_pc);
                self.push_stack(low_pc);
                self.push_stack(self.status | 0b1_0000);

                self.status |= 0b100;

                let new_low_pc = self.read_byte(0xFFFE);
                let new_high_pc = self.read_byte(0xFFFF);

                // println!("new low: {}, new high: {}", new_low_pc, new_high_pc);

                self.pc = (new_high_pc as u16) << 8 | new_low_pc as u16;
            }

            // PHP - Pushes a copy of the status flags onto the stack
            (0, 2, 0) => {
                self.push_stack(self.status | 0b1_0000);
            },

            // CLC - Clears the carry flag
            (0, 6, 0) => {
                self.status &= !0x1;
            },

            // PLP - Pulls a byte of data from the stack and loads it into the status flags
            (1, 2, 0) => {
                let mut popped_value = self.pop_stack();

                popped_value |= 0b10_0000;
                popped_value &= !0b1_0000;
                // println!("popped: {}", popped_value);

                self.status = popped_value;
            },

            // SEC - Sets the carry flag
            (1, 6, 0) => {
                self.status |= 0x1;
            },

            // RTI - Return from interrupt - Pulls processor status flags from stack followed by the program counter
            (2, 0, 0) => {
                self.status = self.pop_stack();
                self.status |= 0b0010_0000; // Sets the unused bit
                self.status &= !0b0001_0000; // Clears the break flag

                let low_pc = self.pop_stack();
                let high_pc = self.pop_stack();

                self.pc = (high_pc as u16) << 8 | low_pc as u16;
            },

            // PHA - Pushes a copy of the accumulator's value onto the stack
            (2, 2, 0) => {
                self.push_stack(self.accumulator);
            },

            // CLI - Clears the interrupt flag (allows normal requests again)
            (2, 6, 0) => {
                self.status &= !0x4;
            }

            // RTS - Return from subroutine - Pulls program counter from stack
            (3, 0, 0) => {
                let low_pc = self.pop_stack() as u16;
                let high_pc = self.pop_stack() as u16;

                self.pc = ((high_pc << 8) | low_pc) + 1;
            },

            // PLA - Pulls a byte value from the stack and loads it into the accumulator
            // Sets the zero/negative flags as appropriate
            (3, 2, 0) => {
                self.accumulator = self.pop_stack();

                self.set_zero_neg(self.accumulator);
            },

            // SEI - Sets the the interrupt disable flag
            (3, 6, 0) => {
                self.status |= 0x4;
            },

            // DEY - Subtracts one from the y register - sets the zero/negative flags as appropriate
            (4, 2, 0) => {
                self.y = self.y.wrapping_sub(1);
                self.set_zero_neg(self.y);
            },

            // TYA - Copies the y register into the accumulator register and sets the zero and negative flags as needed
            (4, 6, 0) => {
                self.accumulator = self.y;
                self.set_zero_neg(self.accumulator);
            },

            // TXA - Copies the x register into the accumulator register and sets the zero and negative flags as needed
            (4, 2, 2) => {
                self.accumulator = self.x;
                self.set_zero_neg(self.accumulator);
            },

            // TXS - Copies the x register into the stack pointer register
            (4, 6, 2) => {
                self.sp = self.x;
            }

            // TAY - Copies the accumulator register to the y register and sets the zero and negative flags as needed
            (5, 2, 0) => {
                self.y = self.accumulator;
                self.set_zero_neg(self.y);
            },

            // CLV - Clears the overflow flag
            (5, 6, 0) => {
                self.status &= !0x40;
            },

            // TAX - Copies the accumulator register to the x register and sets zero and negative flags as needed
            (5, 2, 2) => {
                self.x = self.accumulator;
                self.set_zero_neg(self.x);
            },

            // TSX - Copies the stack pointer value to x and sets the zero and negative flags as needed
            (5, 6, 2) => {
                self.x = self.sp;
                self.set_zero_neg(self.x);
            },

            // INY - Increments the y register by one - sets the zero/negative flags as appropriate
            (6, 2, 0) => {
                self.y = self.y.wrapping_add(1);
                self.set_zero_neg(self.y);
            },

            // DEX - Subtracts one from the x register - sets the zero/negative flags as appropriate
            (6, 2, 2) => {
                self.x = self.x.wrapping_sub(1);
                self.set_zero_neg(self.x);
            },

            // CLD - Clears the decimal flag
            (6, 6, 0) => {
                self.status &= !0x8;
            },

            // INX - Increments the x register by one - sets the zero/negative flags as appropriate
            (7, 2, 0) => {
                self.x = self.x.wrapping_add(1);
                self.set_zero_neg(self.x);
            },

            // NOP - Does noting (literally "No operation" (shocker !!!!!!!!!))
            (7, 2, 2) => {

            },

            // SED - Sets the decimal flag
            (7, 6, 0) => {
                self.status |= 0x8;
            },

            // End implied instructions

            (0, bbb, diff) => {
                match diff {
                    // BPL - Increment the program counter by the relative displacement if the negative flag is clear
                    0 => {
                        let displacement = self.fetch_byte() as i8;
                        
                        if self.status & 0x80 == 0 {
                            self.pc = (displacement as isize + self.pc as isize) as u16;
                        }
                    },

                    // ORA - Logical Inclusive OR - Performed on accumulator with memory-held value
                    // Sets the negative/zero status flags as appropriate
                    1 => {
                        self.accumulator |= self.get_operand(bbb);

                        self.set_zero_neg(self.accumulator);
                    },

                    // ASL - Multiplies the accumulator or a byte from memory by 2
                    // Bit 0 is set to zero and the carry flag is set to bit 7 (if result is >0xFF then carry flag is set)
                    // Has special accumulator mode. Basically immediate, but acts on the accumulator and has other addressing modes
                    2 => {
                        match bbb {
                            // Zero page mode
                            1 => {
                                // println!("Going into asl: {}", self.pc);

                                let zero_page_addr = self.fetch_byte() as u16;
                                let carry_bit = self.read_byte(zero_page_addr) & 0x80;

                                self.left_shift_and_store(zero_page_addr, 1);

                                if carry_bit != 0 {
                                    self.status |= 0x01;
                                } else {
                                    self.status &= !0x01;
                                }

                                let at_addr = self.read_byte(zero_page_addr);
                                self.set_zero_neg(at_addr);
                            },
        
                            // Special Accumulator mode
                            2 => {
                                let carry_bit = self.accumulator & 0x80;
                                self.accumulator <<= 1;
                                
                                if carry_bit != 0 {
                                    self.status |= 0x01;
                                } else {
                                    self.status &= !0x01;
                                }
        
                                self.set_zero_neg(self.accumulator);
                            },
        
                            // Absolute mode
                            3 => {
                                let addr = self.fetch_word();
        
                                let carry_bit = self.read_byte(addr) & 0x80;

                                self.left_shift_and_store(addr, 1);
        
                                if carry_bit != 0 {
                                    self.status |= 0x01;
                                } else {
                                    self.status &= !0x01;
                                }

                                let at_addr = self.read_byte(addr);
                                self.set_zero_neg(at_addr);
                            },

                            // Zero page x mode
                            5 => {
                                let zero_page_addr = (self.fetch_byte().wrapping_add(self.x)) as u16;
                                let carry_bit = self.read_byte(zero_page_addr) & 0x80;

                                self.left_shift_and_store(zero_page_addr, 1);
                                
                                if carry_bit != 0 {
                                    self.status |= 0x01;
                                } else {
                                    self.status &= !0x01;
                                }
        
                                let at_addr = self.read_byte(zero_page_addr);
                                self.set_zero_neg(at_addr);
                            },
        
                            // Absolute x mode
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.x as u16);
        
                                let carry_bit = self.read_byte(addr) & 0x80;

                                self.left_shift_and_store(addr, 1);
        
                                if carry_bit != 0 {
                                    self.status |= 0x01;
                                } else {
                                    self.status &= !0x01;
                                }
        
                                let at_addr = self.read_byte(addr);
                                self.set_zero_neg(at_addr);
                            },
        
                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            },

            (1, bbb, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // JSR - Pushes the address (-1) of the return point onto the stack then sets pc to absolute address
                            0 => {
                                let addr_low = self.fetch_byte();
                                self.push_stack(((self.pc & 0xFF00) >> 8) as u8);
                                self.push_stack((self.pc & 0x00FF) as u8);
                                let addr_high = self.fetch_byte();

                                self.pc = (addr_high as u16) << 8 | addr_low as u16;
                            },

                            // BIT - Checks A & M to set/clear the zero flag, stores bit 7 in the N flag and bit 6 in the V flag
                            1 | 3 => {
                                let operand = self.get_operand(bbb);

                                if operand & 0x80 != 0 {
                                    self.status |= 0x80;
                                } else {
                                    self.status &= !0x80;
                                }

                                if operand & 0x40 != 0 {
                                    self.status |= 0x40;
                                } else {
                                    self.status &= !0x40;
                                }

                                if self.accumulator & operand == 0 {
                                    self.status |= 0x2;
                                } else {
                                    self.status &= !0x2;
                                }
                            },

                            // BMI - Adds the relative displacement to the program counter if the negative flag is set
                            4 => {
                                let displacement = self.fetch_byte() as i8;
                        
                                if self.status & 0x80 > 0 {
                                    self.pc = (displacement as isize + self.pc as isize) as u16;
                                }
                            },

                            _ => { panic!("Opcode not supported!") }
                        }
                    }

                    // AND - Logical AND is performed on the accumulator register and a byte from memory
                    // Sets the zero and negative flags as needed
                    1 => {
                        self.accumulator &= self.get_operand(bbb);
                        self.set_zero_neg(self.accumulator);
                    },

                    // ROL - Move the bits in either A or M one place to the left
                    // Bit 0 is filled with the carry flag, bit 7 becomes the new carry flag
                    2 => {
                        match bbb {
                            // Zero page mode
                            1 => {
                                let old_carry = self.status & 0x1;
                                let zero_page_addr = self.read_byte(self.pc) as u16;
                                let new_carry = self.get_operand(bbb) & 0x80;

                                self.left_shift_and_store(zero_page_addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(zero_page_addr, 0x1);
                                } else {
                                    self.and_and_store(zero_page_addr, !0x1);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            }

                            // Special accumulator mode
                            2 => {
                                let old_carry = self.status & 0x1;
                                let new_carry = self.accumulator & 0x80;
                                self.accumulator <<= 1;

                                if old_carry != 0 {
                                    self.accumulator |= 0x1;
                                } else {
                                    self.accumulator &= !0x1;
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg(self.accumulator);
                            },

                            // Absolute mode
                            3 => {
                                let old_carry = self.status & 0x1;
                                let addr = self.cpu_bus.mem_read_u16(self.pc);
                                let new_carry = self.get_operand(bbb) & 0x80;

                                self.left_shift_and_store(addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(addr, 0x1);
                                } else {
                                    self.and_and_store(addr, !0x1);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            // Zero page x mode
                            5 => {
                                let old_carry = self.status & 0x1;
                                let zero_page_addr = self.fetch_byte().wrapping_add(self.x) as u16;
                                let new_carry = self.read_byte(zero_page_addr) & 0x80;

                                self.left_shift_and_store(zero_page_addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(zero_page_addr, 0x1);
                                } else {
                                    self.and_and_store(zero_page_addr, !0x1);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            },

                            // Absolute x mode
                            7 => {
                                let old_carry = self.status & 0x1;
                                let addr = self.cpu_bus.mem_read_u16(self.pc).wrapping_add(self.x as u16);
                                let new_carry = self.get_operand(bbb) & 0x80;

                                self.left_shift_and_store(addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(addr, 0x1);
                                } else {
                                    self.and_and_store(addr, !0x1);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            _ => { panic!("Opcode not supported!") }   
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            },

            (2, _, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // JMP Absolute - Sets the program counter to the address specified by memory
                            3 => {
                                let addr = self.fetch_word();

                                self.pc = addr;
                            },
            
                            // BVC - Increments the program counter by the relative displacement if the overflow flag is clear
                            4 => {
                                let displacement: i8 = self.fetch_byte() as i8;
                                if self.status & 0x40 == 0 {
                                    self.pc = (displacement as i64 + self.pc as i64) as u16
                                }
                            },

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    // EOR - Performs an exclusive OR operation on a memory-held value and the accumulator
                    // Sets the zero/negative flags as appropriate
                    1 => {
                        self.accumulator ^= self.get_operand(bbb);
                        self.set_zero_neg(self.accumulator);
                    },

                    // LSR - Shifts all the bits of the operand to the right
                    // The bit in bit 0 is sent to the carry flag, bit 7 is set to 0
                    2 => {
                        match bbb {
                            // Zero page mode
                            1 => {
                                let zero_page_addr = self.fetch_byte() as u16;
                                let zero_bit = self.read_byte(zero_page_addr) & 0x1;

                                self.right_shift_and_store(zero_page_addr, 1);

                                if zero_bit != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            },

                            // Special accumulator mode
                            2 => {
                                let zero_bit = self.accumulator & 0x1;
                                self.accumulator >>= 1;

                                if zero_bit != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg(self.accumulator);
                            },

                            // Absolute mode
                            3 => {
                                let addr = self.fetch_word();
                                let zero_bit = self.read_byte(addr) & 0x1;

                                self.right_shift_and_store(addr, 1);

                                if zero_bit != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            // Zero page x mode
                            5 => {
                                let zero_page_addr = (self.fetch_byte().wrapping_add(self.x)) as u16;
                                let zero_bit = self.read_byte(zero_page_addr) & 0x1;

                                self.right_shift_and_store(zero_page_addr, 1);

                                if zero_bit != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            },

                            // Absolute x mode
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.x as u16);
                                let zero_bit = self.read_byte(addr) & 0x1;

                                self.right_shift_and_store(addr, 1);

                                if zero_bit != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            }

            (3, _, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // JMP Indirect - Jumps to the specified address (absolute address that points to lsb of operand)
                            3 => {
                                self.indirect();
                            },

                            // BVS - Increment the program counter by the relative displacement if the overflow flag is set
                            4 => {
                                let displacement: i8 = self.fetch_byte() as i8;
                                if self.status & 0x40 != 0 {
                                    self.pc = (displacement as i64 + self.pc as i64) as u16;
                                }
                            },

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    // ADC - Adds the contents of a memory location to the accumulator together with the carry bit
                    // If overflow occurs the carry bit is set - this allows for multi-byte addition
                    // Sets the zero and negative flags as needed, also sets overflow flag if sign bit is incorrect
                    1 => {
                        let operand= self.get_operand(bbb);

                        let carry_bit = self.status & 0x1;
                        let previous = self.accumulator;
                        let result = self.accumulator as u16 + operand as u16 + carry_bit as u16;

                        if result > 255 { self.status |= 0x1 } else { self.status &= !0x01 };

                        self.accumulator = result as u8;
                        self.set_zero_neg(self.accumulator);

                        let prev_sign = previous & 0x80;
                        let op_sign = operand & 0x80;
                        let res_sign = self.accumulator & 0x80;

                        if prev_sign == op_sign && prev_sign != res_sign {
                            self.status |= 0x40;
                        } else {
                            self.status &= !0x40;
                        }
                    },

                    // ROR - Move the bits in either A or M one place to the right
                    // Bit 7 is filled with the carry flag, bit 0 becomes the new carry flag
                    2 => {
                        match bbb {
                            // Zero page mode
                            1 => {
                                let old_carry = self.status & 0x1;
                                let zero_page_addr = self.fetch_byte() as u16;
                                let new_carry = self.read_byte(zero_page_addr) & 0x1;

                                self.right_shift_and_store(zero_page_addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(zero_page_addr, 0x80);
                                } else {
                                    self.and_and_store(zero_page_addr, !0x80);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            }

                            // Special accumulator mode
                            2 => {
                                let old_carry = self.status & 0x1;
                                let new_carry = self.accumulator & 0x1;
                                self.accumulator >>= 1;

                                if old_carry != 0 {
                                    self.accumulator |= 0x80;
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg(self.accumulator);
                            },

                            // Absolute mode
                            3 => {
                                let old_carry = self.status & 0x1;
                                let addr = self.fetch_word();
                                let new_carry = self.read_byte(addr) & 0x1;

                                self.right_shift_and_store(addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(addr, 0x80);
                                } else {
                                    self.and_and_store(addr, !0x80);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            // Zero page x mode
                            5 => {
                                let old_carry = self.status & 0x1;
                                let zero_page_addr = (self.fetch_byte().wrapping_add(self.x)) as u16;
                                let new_carry = self.read_byte(zero_page_addr) & 0x1;

                                self.right_shift_and_store(zero_page_addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(zero_page_addr, 0x80);
                                } else {
                                    self.and_and_store(zero_page_addr, !0x80);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(zero_page_addr);
                            },

                            // Absolute x mode
                            7 => {
                                let old_carry = self.status & 0x1;
                                let addr = self.fetch_word().wrapping_add(self.x as u16);
                                let new_carry = self.read_byte(addr) & 0x1;

                                self.right_shift_and_store(addr, 1);

                                if old_carry != 0 {
                                    self.or_and_store(addr, 0x80);
                                } else {
                                    self.and_and_store(addr, !0x80);
                                }

                                if new_carry != 0 {
                                    self.status |= 0x1;
                                } else {
                                    self.status &= !0x1;
                                }

                                self.set_zero_neg_flags(addr);
                            },

                            _ => { panic!("Opcode not supported!") }   
                        }
                    }

                    _ => { panic!("Opcode not implemented!") }
                }
            },

            // Store instructions
            (4, bbb, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // Zero-page addressing - STY
                            1 => {
                                let addr = self.fetch_byte() as u16;

                                self.write_byte(addr, self.y);
                            },

                            // Absolute addressing - STY
                            3 => {
                                let addr = self.fetch_word();

                                self.write_byte(addr, self.y);
                            },

                            // Zero-page y addressing - STY
                            5 => {
                                let addr = (self.fetch_byte().wrapping_add(self.x)) as u16;

                                self.write_byte(addr, self.y);
                            },

                            // BCC - Adds the relative displacement to the program counter if the carry flag is clear
                            4 => {
                                let displacement = self.fetch_byte() as i8;
                                if self.status & 0x01 == 0 {
                                    self.pc = (displacement as i64 + self.pc as i64) as u16;
                                }
                            },

                           _ => { panic!("Opcode not supported!") } 
                        }
                    },

                    // STA - Stores the accumulator register into memory
                    1 => {
                        match bbb {
                            // Indexed Indirect addressing
                            0 => {
                                let zpage_addr = self.fetch_byte().wrapping_add(self.x);
                                let lsb = self.read_byte(zpage_addr as u16);
                                let msb = self.read_byte(zpage_addr.wrapping_add(1) as u16);
                                let addr = (msb as u16) << 8 | lsb as u16;

                                self.write_byte(addr, self.accumulator);
                            },

                            // Zero-page addressing
                            1 => {
                                let addr = self.fetch_byte() as u16;

                                self.write_byte(addr, self.accumulator);
                            },

                            // Absolute addressing
                            3 => {
                                let addr = self.fetch_word();
                                //println!("addr: {}", addr);

                                self.write_byte(addr, self.accumulator);
                            },

                            // Indirect Indexed addressing
                            4 => {
                                let zpage_addr = self.fetch_byte();
                                let lsb = self.read_byte(zpage_addr as u16);
                                let msb = self.read_byte(zpage_addr.wrapping_add(1) as u16);
                                let addr = ((msb as u16) << 8 | lsb as u16).wrapping_add(self.y as u16);

                                self.write_byte(addr, self.accumulator);
                            },

                            // Zero-page x addressing
                            5 => {
                                let addr = (self.fetch_byte().wrapping_add(self.x)) as u16;

                                self.write_byte(addr, self.accumulator);
                            },

                            // Absolute y addressing
                            6 => {
                                let addr = self.fetch_word().wrapping_add(self.y as u16);

                                self.write_byte(addr, self.accumulator);
                            },

                            // Absolute x addressing
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.x as u16);

                                self.write_byte(addr, self.accumulator);
                            }

                            _ => { panic!("Opcode not supported") }
                        }
                    },

                    // STX - Stores the x register into memory
                    2 => {
                        match bbb {
                            // Zero-page addressing
                            1 => {
                                let addr = self.fetch_byte() as u16;

                                self.write_byte(addr, self.x);
                            },

                            // Absolute addressing
                            3 => {
                                let addr = self.fetch_word();

                                self.write_byte(addr, self.x);
                            },

                            // Zero-page y addressing
                            5 => {
                                let addr = (self.fetch_byte().wrapping_add(self.y)) as u16;

                                self.write_byte(addr, self.x);
                            },

                            _ => { panic!("Opcode not supported") }
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            }

            // Load Instructions
            (5, bbb, diff) => { 
                match diff {
                    0 => match bbb {
                        // LDY - Loads a byte from memory into the y register and changes the zero and negative flags as needed
                        // LDY - Immediate
                        0 => {
                            self.y = self.fetch_byte();
                            self.set_zero_neg(self.y);
                        },

                        // LDY - Zero-page
                        1 => {
                            let zero_page_addr = self.fetch_byte() as u16;
                            self.y = self.read_byte(zero_page_addr);
                            self.set_zero_neg(self.y);
                        }

                        // LDY - Absolute
                        3 => {
                            let addr = self.fetch_word();
                            self.y = self.read_byte(addr);
                            self.set_zero_neg(self.y);
                        }

                        // LDY - Zero-page x
                        5 => {
                            let zero_page_addr = (self.fetch_byte().wrapping_add(self.x)) as u16;
                            self.y = self.read_byte(zero_page_addr);
                            self.set_zero_neg(self.y);
                        }

                        // LDY - Absolute x
                        7 => {
                            let addr = self.fetch_word().wrapping_add(self.x as u16);
                            self.y = self.read_byte(addr);
                            self.set_zero_neg(self.y);
                        }

                        // BCS - Adds the relative displacement to the program counter if the carry flag is clear
                        4 => {
                            let displacement = self.fetch_byte() as i8;
                            if self.status & 0x01 != 0 {
                                self.pc = (displacement as i64 + self.pc as i64) as u16;
                            }
                        },

                        _ => { panic!("Opcode not supported!") }
                    }

                    // LDA - Loads a byte from memory into the accumulator and changes the zero and negative flags as needed
                    1 => {
                        self.accumulator = self.get_operand(bbb);
                        self.set_zero_neg(self.accumulator);
                    },

                    // LDX - Loads a byte from memory into the x register and changes the zero and negative flags as needed
                    2 => {
                        match bbb {
                            // LDX - Loads a byte from memory into the y register and changes the zero and negative flags as needed
                            // LDX - Immediate
                            0 => {
                                self.x = self.fetch_byte();
                                self.set_zero_neg(self.x);
                            },
    
                            // LDX - Zero-page
                            1 => {
                                let zero_page_addr = self.fetch_byte() as u16;
                                self.x = self.read_byte(zero_page_addr);
                                self.set_zero_neg(self.x);
                            }
    
                            // LDX - Absolute
                            3 => {
                                let addr = self.fetch_word();
                                self.x = self.read_byte(addr);
                                self.set_zero_neg(self.x);
                            }
    
                            // LDX - Zero-page y
                            5 => {
                                let zero_page_addr = (self.fetch_byte().wrapping_add(self.y)) as u16;
                                self.x = self.read_byte(zero_page_addr);
                                self.set_zero_neg(self.x);
                            },

                            // LDX - Absolute y
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.y as u16);
                                self.x = self.read_byte(addr);
                                self.set_zero_neg(self.x);
                            },

                            _ => panic!("Opcode not supported!")
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            }

            (6, bbb, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // CPY - Compares the y register with another memory held value
                            // Sets the zero/negative/carry flags as appropriate
                            0 => {
                                let operand = self.fetch_byte();

                                self.set_cmp_flags(self.y, operand);
                            },

                            // 1 & 3 required for same reason as CPX
                            1 => {
                                let operand = self.get_operand(bbb);
                                self.set_cmp_flags(self.y, operand);
                            },

                            3 => {
                                let operand = self.get_operand(bbb);
                                self.set_cmp_flags(self.y, operand);
                            },

                            // BNE - Increment the program counter by the relative displacement if the zero counter is clear
                            4 => {
                                let displacement = self.fetch_byte() as i8;
                                if self.status & 0x2 == 0 {
                                    self.pc = (displacement as i64 + self.pc as i64) as u16;
                                }
                            },

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    // CMP - Compares a value from memory with the accumulator register
                    // Sets zero/carry/negative flags as appropriate
                    1 => {
                        let operand = self.get_operand(bbb);
                        self.set_cmp_flags(self.accumulator, operand);
                    },

                    // DEC - Subtracts one from a memory-held value - Sets the negative and zero flags as appropriate
                    // Because DEC directly changes memory values we need to code each addressing mode
                    2 => {
                        match bbb {
                            // Zero-page
                            1 => {
                                let addr = self.fetch_byte();

                                self.dec(addr as u16);
                            },

                            // Absolute
                            3 => {
                                let addr = self.fetch_word();

                                self.dec(addr as u16)
                            },

                            // Zero-page x
                            5 => {
                                let addr = self.fetch_byte().wrapping_add(self.x);

                                self.dec(addr as u16);
                            },

                            // Absolute-x
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.x as u16);

                                self.dec(addr);
                            }

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    _ => { panic!("Opcode not supported!") }
                }
            },

            (7, bbb, diff) => {
                match diff {
                    0 => {
                        match bbb {
                            // CPX - Compares the x register with another memory held value
                            // Sets the zero/negative/carry flags as appropriate
                            0 => {
                                let operand = self.fetch_byte();

                                self.set_cmp_flags(self.x, operand);
                            },

                            // 1 & 3 Neccessary because CPX and CPY are annoying in that immediate bbb is 2 not 4
                            1 => {
                                let operand = self.get_operand(bbb);
                                self.set_cmp_flags(self.x, operand);
                            },

                            3 => {
                                let operand = self.get_operand(bbb);
                                self.set_cmp_flags(self.x, operand);
                            }

                            // BEQ - Increment the program counter by the relative displacement if the zero counter is set
                            4 => {
                                let displacement = self.fetch_byte() as i8;
                                if self.status & 0x2 != 0 {
                                    self.pc = (displacement as i64 + self.pc as i64) as u16;
                                }
                            },

                            _ => { panic!("Opcode not implemented!") }
                        }
                    },

                    // SBC - Subtracts the contents of a memory location from the accumulator together with the not of the carry flag
                    // If overflow occurs the carry bit is clear - Allows for multi-byte subtraction
                    1 => {
                        let carry_bit = self.status & 0x1;
                        let prev = self.accumulator;
                        let operand = self.get_operand(bbb);
                        self.accumulator = self.accumulator.wrapping_sub(operand).wrapping_sub(1 - carry_bit);

                        let sign = self.accumulator & 0x80;
                        let op_sign = operand & 0x80;
                        let prev_sign = prev & 0x80;

                        // println!("after accumulator: {}", self.accumulator);
                        // println!("prev: {}, carry_bit: {}, operand: {}, term: {}", prev, carry_bit, operand, operand.wrapping_add(1-carry_bit));

                        if (prev as u16) < (operand as u16 + (1 - carry_bit as u16)) {
                            self.status &= !0x01;
                        } else {
                            self.status |= 0x01;
                        }

                        if op_sign != prev_sign && prev_sign != sign {
                            self.status |= 0x40;
                        } else {
                            self.status &= !0x40;
                        }

                        self.set_zero_neg(self.accumulator);
                    },

                    // INC - Adds one to a memory-held value - Sets the negative and zero flags as appropriate
                    // Because INC directly changes memory values we need to code each addressing mode
                    2 => {
                        match bbb {
                            // Zero-page
                            1 => {
                                let addr = self.fetch_byte();

                                self.inc(addr as u16);
                            },

                            // Absolute
                            3 => {
                                let addr = self.fetch_word();

                                self.inc(addr as u16)
                            },

                            // Zero-page x
                            5 => {
                                let addr = self.fetch_byte().wrapping_add(self.x);

                                self.inc(addr as u16);
                            },

                            // Absolute-x
                            7 => {
                                let addr = self.fetch_word().wrapping_add(self.x as u16);

                                self.inc(addr);
                            }

                            _ => { panic!("Opcode not supported!") }
                        }
                    },

                    _ => { panic!("Opcode not implemented!") }
                }
            },

            _ => { panic!("Opcode not supported!") }
        }
    }

    fn set_zero_neg(&mut self, check: u8) {
        // If the input is zero, the seventh status register bit (zero check) is set
        if check == 0 {
            self.status |= 0x2;
        } else {
            self.status &= !0x2;
        }
        
        // If the input's seventh bit is set, the first status register bit (negative check) is set
        if check & 0x80 != 0 {
            self.status |= 0x80;
        } else {
            self.status &= !0x80;
        }
    }

    fn set_cmp_flags(&mut self, check: u8, operand: u8) {
        // If the register is >= to the memory stored value, the carry flag is set
        if check >= operand {
            self.status |= 0x1;
        } else {
            self.status &= !0x1;
        }

        // If the register is equal to the memory stored value, the seventh status register bit (zero check) is set
        if check == operand {
            self.status |= 0x2;
        } else {
            self.status &= !0x2;
        }

        // If the input's seventh bit is set, the first status register bit (negative check) is set
        if (check.wrapping_sub(operand)) & 0x80 != 0 {
            self.status |= 0x80;
        } else {
            self.status &= !0x80;
        }
    }

    // Addressing modes - Helper functions that return values for the CPU to run instructions against
    // For example, the immediate address mode will always operate on the next byte in memory
    fn immediate(&mut self) -> u8 {
        self.fetch_byte()
    }

    // Absolute addressing - returns the value specified in a 16 bit address
    fn absolute(&mut self) -> u8 {
        let addr = self.fetch_word();
        self.read_byte(addr)
    }

    // Zero page addressing - returns the value specified in the first 256 bytes - requires one less byte
    fn zero_page(&mut self) -> u8 {
        let zero_page_addr = self.fetch_byte();
        self.read_byte(zero_page_addr as u16)
    }

    // Zero page x addressing - adds the value of register x to the next byte and retrieves the value
    fn zero_page_x(&mut self) -> u8 {
        let zero_page_addr = self.fetch_byte().wrapping_add(self.x);
        self.read_byte(zero_page_addr as u16)
    }

    // Zero page y addressing - adds the value of register y to the next byte and retrieves the value
    fn zero_page_y(&mut self) -> u8 {
        let zero_page_addr = self.fetch_byte().wrapping_add(self.y);
        self.read_byte(zero_page_addr as u16)
    }

    // Absolute addressing with x - returns the value at the absolute address plus the value in the x register
    fn absolute_x(&mut self) -> u8 {
        let addr = self.fetch_word().wrapping_add(self.x as u16);
        self.read_byte(addr)
    }

    // Absolute addressing with y - returns the value at the absolute address plus the value in the y register
    fn absolute_y(&mut self) -> u8 {
        let addr = self.fetch_word().wrapping_add(self.y as u16);
        self.read_byte(addr)
    }

    // Indirect addressing - Changes the pc to the next two bytes of the address provided (provides the lsb)
    // Quirk - If at page boundary the 6502 will actually take xx00 after xxFF instead of 0(x+1)00
    fn indirect(&mut self) {
        let word = self.fetch_word();
        let lsb = self.read_byte(word);
        let msb = if word & 0x00FF == 0x00FF {
            self.read_byte(word & 0xFF00)
        } else {
            self.read_byte(word + 1)
        };
        self.pc = (msb as u16) << 8 | lsb as u16;
    }

    // Indexed Indirect - Takes a zero page address, adds the value of x to it --> new address of lsb, combine with next byte to form new address
    fn indexed_indirect(&mut self) -> u8 {
        let zpage_addr = self.fetch_byte().wrapping_add(self.x);
        let lsb = self.read_byte(zpage_addr as u16);
        let msb = self.read_byte(zpage_addr.wrapping_add(1) as u16);

        self.read_byte((msb as u16) << 8 | lsb as u16)
    }

    // Indirect Indexed - Slightly more sane younger brother
    // Take a zero page address, find the msb/lsb to get new address, add y to that new address
    fn indirect_indexed(&mut self) -> u8 {
        let zpage_addr = self.fetch_byte();
        let lsb = self.read_byte(zpage_addr as u16);
        let msb = self.read_byte(zpage_addr.wrapping_add(1) as u16);
        let addr = ((msb as u16) << 8 | lsb as u16).wrapping_add(self.y as u16);

        self.read_byte(addr)
    }

    fn get_operand(&mut self, bbb_data: u8) -> u8 {
        match bbb_data {
            0 => { self.indexed_indirect() },
            1 => { self.zero_page() },
            2 => { self.immediate() },
            3 => { self.absolute() },
            4 => { self.indirect_indexed() },
            5 => { self.zero_page_x() },
            6 => { self.absolute_y() },
            7 => { self.absolute_x() },
            _ => { panic!("Addressing mode not supported") }, 
        }
    }

    // Push and pop functions for the stack
    fn push_stack(&mut self, data: u8) {
        let stack_addr = self.sp as u16 + STACK_BASE as u16;
        self.write_byte(stack_addr, data);

        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop_stack(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let stack_addr = self.sp as u16 + STACK_BASE as u16;
        let rtrn_value = self.read_byte(stack_addr);
        // println!("return: {}", rtrn_value);

        rtrn_value
    }
}
