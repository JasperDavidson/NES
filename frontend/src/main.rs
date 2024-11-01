use core::time;
use std::{fs::File, io::{BufReader, Read}, thread};
use serde_json::{Value, Result};
use minifb;

use nes_components::*;

pub fn nes_start() {

}

// pub fn nes_tick(cpu: &mut CPU) {
// }

fn main() -> Result<()> {
    let mut nes_file = match File::open("C:/Users/Jasper Davidson/Documents/Programming/Rust/nes/frontend/roms/donkey_kong.nes") {
        Ok(file) => file,
        Err(error) => { panic!("Problem opening ROM file: {:?}", error) },
    };

    let palette_buffer = std::fs::read("C:/Users/Jasper Davidson/Documents/Programming/Rust/nes/frontend/palettes/ntsc_palette.pal").expect("Unable to read palette file");

    let mut buffer: Vec<u8> = vec![];

    nes_file.read_to_end(&mut buffer).expect("Could not load the rom");

    let rom = match Rom::new(&buffer) {
        Ok(rom) => rom,
        Err(e) => panic!("Error: {}", e)
    };

    // CPU testing json extraction
    // let json_path = r"C:\Users\Jasper Davidson\Documents\Programming\Rust\nes\frontend\tests\6502SstepTests\16.json";

    // // Open the file
    // let json_file = File::open(json_path).expect("file should open read only");

    // // Read the contents of the file
    // let mut contents = String::new();
    // let mut reader = BufReader::new(json_file);
    // reader.read_to_string(&mut contents).expect("Failed to read the file to string");

    // // Try to parse the JSON and print detailed errors
    // let json: Value = serde_json::from_str(&contents)?;

    let mut window = minifb::Window::new(
        "NES",
        SCREEN_WIDTH,
        SCREEN_HEIGHT,
        minifb::WindowOptions {
            scale: minifb::Scale::X2,
            ..Default::default()
        }
    ).unwrap_or_else(|e| panic!("{}", e));

    window.set_target_fps(60);

    // Testing loop for CPU Instructions
    // for i in 0..10000 {
    //     let (pc, status, a, x, y, sp) = (json[i]["initial"]["pc"].as_i64().expect("Could not extract pc"), 
    //                                                                     json[i]["initial"]["p"].as_i64().expect("Could not extract status"),
    //                                                                     json[i]["initial"]["a"].as_i64().expect("Could not extract a"),
    //                                                                     json[i]["initial"]["x"].as_i64().expect("Could not extract x"),
    //                                                                     json[i]["initial"]["y"].as_i64().expect("Could not extract x"),
    //                                                                     json[i]["initial"]["s"].as_i64().expect("Could not extract sp"));

    //     let (fpc, fstatus, fa, fx, fy, fsp) = (json[i]["final"]["pc"].as_i64().expect("Could not extract pc"), 
    //                                                                         json[i]["final"]["p"].as_i64().expect("Could not extract status"),
    //                                                                         json[i]["final"]["a"].as_i64().expect("Could not extract a"),
    //                                                                         json[i]["final"]["x"].as_i64().expect("Could not extract x"),
    //                                                                         json[i]["final"]["y"].as_i64().expect("Could not extract x"),
    //                                                                         json[i]["final"]["s"].as_i64().expect("Could not extract sp"));

    //     let ram_initial: Vec<(i64, i64)> = json[i]["initial"]["ram"].as_array().expect("Could not load ram")
    //                                 .iter().map(|x| {
    //                                     let pair = x.as_array().expect("Expected array of pairs");
    //                                     let addr = pair[0].as_i64().expect("Expected address as i64");
    //                                     let value = pair[1].as_i64().expect("Expected value as i64");
    //                                     (addr, value)
    //                                 })
    //                                 .collect();

    //     let ram_final: Vec<(i64, i64)> = json[i]["final"]["ram"].as_array().expect("Could not load ram")
    //                                 .iter().map(|x| {
    //                                     let pair = x.as_array().expect("Expected array of pairs");
    //                                     let addr = pair[0].as_i64().expect("Expected address as i64");
    //                                     let value = pair[1].as_i64().expect("Expected value as i64");
    //                                     (addr, value)
    //                                 })
    //                                 .collect();
                                
    //     let ppu = PPU::init_ppu(rom.chr_rom.clone(), rom.screen_mirroring, palette_buffer.clone());
    //     let mut cpu = CPU::init_cpu(a as u8, x as u8, y as u8, pc as u16, sp as u8, status as u8, rom.clone().prg_rom, ppu);
    //     cpu.load_testing_ram(&ram_initial);

    //     loop {
    //         cpu.decode();

    //         let mut ram_equal = true;

    //         for (addr, value) in &ram_final {
    //             if cpu.read_byte(*addr as u16) != *value as u8 {
    //                 ram_equal = false;
    //             }
    //         }

    //         if (cpu.pc, cpu.status, cpu.accumulator, cpu.x, cpu.y, cpu.sp) == (fpc as u16, fstatus as u8, fa as u8, fx as u8, fy as u8, fsp as u8)
    //             && ram_equal {
    //             println!("success {}!", i + 2);
    //             break;
    //         }

    //         if cpu.pc == 0 {
    //             panic!("Infinite loop");
    //         }
    //     }
    // }

    let ppu = PPU::init_ppu(rom.chr_rom.clone(), rom.screen_mirroring, palette_buffer.clone(), window);
    let mut cpu = CPU::init_cpu(rom.clone().prg_rom, ppu);

    loop {
        cpu.decode();
        //println!("dot: {}", cpu.cpu_bus.ppu.state.dots);
        println!("scanline: {}", cpu.cpu_bus.ppu.state.scanline);
    }


    Ok(())
}

// // Testing suite to ensure proper addressing/instruction functionality
// #[cfg(test)]
// mod tests {
//     use core::panic;

//     use nes_cpu::*;

//     // Tests for the 6502 addressing modes based on the LDA instruction
//     #[test]
//     fn test_immediate() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xA9);
//         cpu.write_byte(1, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     #[test]
//     fn test_absolute() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xAD);
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.write_byte((12 * 256) + 34, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     #[test]
//     fn test_zero_page() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xA5);
//         cpu.write_byte(1, 50);
//         cpu.write_byte(50, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     // Same principle for zero_page_y
//     #[test]
//     fn test_zero_page_x() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xB5);
//         cpu.write_byte(1, 50);
//         cpu.x = 5;
//         cpu.write_byte(55, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     // Same principle for absolute_y
//     #[test]
//     fn absolute_x() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xBD);
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.x = 10;
//         cpu.write_byte((12 * 256) + 34 + 10, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     #[test]
//     fn indexed_indirect() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xA1);
//         cpu.write_byte(1, 50);
//         cpu.x = 10;
//         cpu.write_byte(60, 34);
//         cpu.write_byte(61, 12);
//         cpu.write_byte((12 * 256) + 34, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     #[test]
//     fn indirect_indexed() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0xB1);
//         cpu.write_byte(1, 50);
//         cpu.write_byte(50, 34);
//         cpu.write_byte(51, 12);
//         cpu.y = 10;
//         cpu.write_byte((12 * 256) + 34 + 10, 1);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 1);
//     }

//     #[test]
//     fn adc() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);

//         cpu.write_byte(0, 0x69);
//         cpu.accumulator = 100;
//         cpu.write_byte(1, 200);
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 44);
//         assert_eq!(cpu.status, 0b00100001);
//     }

//     #[test]
//     fn asl() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
        
//         cpu.write_byte(0, 0x1E);
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.x = 5;
//         cpu.write_byte((12 * 256) + 34 + 5, 150);
//         cpu.decode();

//         assert_eq!(cpu.read_byte((12 * 256) + 34 + 5), 44);
//         assert_eq!(cpu.status, 0b00100001);
//     }

//     #[test]
//     fn bcc() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 500;

//         cpu.write_byte(500, 0x90);
//         cpu.write_byte(501, 200);
//         cpu.decode();
        
//         assert_eq!(cpu.pc, 446);
//     }

//     #[test]
//     fn cmp() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0xEC);
//         cpu.x = 2;
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.write_byte((12*256) + 34, 7);
//         cpu.decode();

//         assert_eq!(cpu.status, 0b10100000);
//     }

//     #[test]
//     fn dec() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0xDE);
//         cpu.x = 5;
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.write_byte((12*256) + 34 + cpu.x as u16, 10);
//         cpu.decode();

//         assert_eq!(cpu.read_byte((12*256) + 34 + cpu.x as u16), 9);
//     }

//     #[test]
//     fn inc() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0xFE);
//         cpu.x = 5;
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.write_byte((12*256) + 34 + cpu.x as u16, 10);
//         cpu.decode();

//         assert_eq!(cpu.read_byte((12*256) + 34 + cpu.x as u16), 11);
//     }

//     #[test]
//     fn jmp() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0x6C);
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.write_byte((12*256) + 34, 10);
//         cpu.decode();

//         assert_eq!(cpu.pc, 10);
//     }

//     #[test]
//     fn jsr() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0x20);
//         cpu.write_byte(1, 10);
//         cpu.write_byte(2, 0);
//         cpu.decode();

//         assert_eq!(cpu.pc, 10);
//         assert_eq!(cpu.read_byte(cpu.sp as u16 + 1 + STACK_BASE as u16), 2)
//     }

//     #[test]
//     fn lsr() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0x5E);
//         cpu.write_byte(1, 34);
//         cpu.write_byte(2, 12);
//         cpu.x = 5;
//         cpu.write_byte((12*256) + 34 + 5, 255);
//         cpu.decode();

//         assert_eq!(cpu.read_byte((12*256) + 34 + 5), 127);
//         assert_eq!(cpu.status, 0b00100001);
//     }

//     #[test]
//     fn sbc() {
//         let rom = match Rom::new(&vec![0]) {
//             Ok(rom) => rom,
//             Err(e) => panic!("Error: {}", e)
//         };

//         let mut cpu = Cpu::init_cpu(rom);
//         cpu.pc = 0;

//         cpu.write_byte(0, 0xE9);
//         cpu.write_byte(1, 15);
//         cpu.accumulator = 10;
//         cpu.decode();

//         assert_eq!(cpu.accumulator, 250);
//         assert_eq!(cpu.status, 0b11100000);
//     }
// }
