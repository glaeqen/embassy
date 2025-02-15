use super::bd::BackupDomain;
pub use super::bus::{AHBPrescaler, APBPrescaler};
use super::RtcClockSource;
pub use crate::pac::pwr::vals::Vos as VoltageScale;
use crate::pac::rcc::vals::{Hpre, Msirange, Plldiv, Pllmul, Pllsrc, Ppre, Sw};
#[cfg(crs)]
use crate::pac::{crs, CRS, SYSCFG};
use crate::pac::{FLASH, PWR, RCC};
use crate::rcc::{set_freqs, Clocks};
use crate::time::Hertz;

/// HSI speed
pub const HSI_FREQ: Hertz = Hertz(16_000_000);

/// LSI speed
pub const LSI_FREQ: Hertz = Hertz(32_000);

/// System clock mux source
#[derive(Clone, Copy)]
pub enum ClockSrc {
    MSI(MSIRange),
    PLL(PLLSource, PLLMul, PLLDiv),
    HSE(Hertz),
    HSI16,
}

/// MSI Clock Range
///
/// These ranges control the frequency of the MSI. Internally, these ranges map
/// to the `MSIRANGE` bits in the `RCC_ICSCR` register.
#[derive(Clone, Copy)]
pub enum MSIRange {
    /// Around 65.536 kHz
    Range0,
    /// Around 131.072 kHz
    Range1,
    /// Around 262.144 kHz
    Range2,
    /// Around 524.288 kHz
    Range3,
    /// Around 1.048 MHz
    Range4,
    /// Around 2.097 MHz (reset value)
    Range5,
    /// Around 4.194 MHz
    Range6,
}

impl Default for MSIRange {
    fn default() -> MSIRange {
        MSIRange::Range5
    }
}

/// PLL divider
#[derive(Clone, Copy)]
pub enum PLLDiv {
    Div2,
    Div3,
    Div4,
}

/// PLL multiplier
#[derive(Clone, Copy)]
pub enum PLLMul {
    Mul3,
    Mul4,
    Mul6,
    Mul8,
    Mul12,
    Mul16,
    Mul24,
    Mul32,
    Mul48,
}

/// PLL clock input source
#[derive(Clone, Copy)]
pub enum PLLSource {
    HSI16,
    HSE(Hertz),
}

impl From<PLLMul> for Pllmul {
    fn from(val: PLLMul) -> Pllmul {
        match val {
            PLLMul::Mul3 => Pllmul::MUL3,
            PLLMul::Mul4 => Pllmul::MUL4,
            PLLMul::Mul6 => Pllmul::MUL6,
            PLLMul::Mul8 => Pllmul::MUL8,
            PLLMul::Mul12 => Pllmul::MUL12,
            PLLMul::Mul16 => Pllmul::MUL16,
            PLLMul::Mul24 => Pllmul::MUL24,
            PLLMul::Mul32 => Pllmul::MUL32,
            PLLMul::Mul48 => Pllmul::MUL48,
        }
    }
}

impl From<PLLDiv> for Plldiv {
    fn from(val: PLLDiv) -> Plldiv {
        match val {
            PLLDiv::Div2 => Plldiv::DIV2,
            PLLDiv::Div3 => Plldiv::DIV3,
            PLLDiv::Div4 => Plldiv::DIV4,
        }
    }
}

impl From<PLLSource> for Pllsrc {
    fn from(val: PLLSource) -> Pllsrc {
        match val {
            PLLSource::HSI16 => Pllsrc::HSI16,
            PLLSource::HSE(_) => Pllsrc::HSE,
        }
    }
}

impl From<MSIRange> for Msirange {
    fn from(val: MSIRange) -> Msirange {
        match val {
            MSIRange::Range0 => Msirange::RANGE0,
            MSIRange::Range1 => Msirange::RANGE1,
            MSIRange::Range2 => Msirange::RANGE2,
            MSIRange::Range3 => Msirange::RANGE3,
            MSIRange::Range4 => Msirange::RANGE4,
            MSIRange::Range5 => Msirange::RANGE5,
            MSIRange::Range6 => Msirange::RANGE6,
        }
    }
}

/// Clocks configutation
pub struct Config {
    pub mux: ClockSrc,
    pub ahb_pre: AHBPrescaler,
    pub apb1_pre: APBPrescaler,
    pub apb2_pre: APBPrescaler,
    #[cfg(crs)]
    pub enable_hsi48: bool,
    pub rtc: Option<RtcClockSource>,
    pub lse: Option<Hertz>,
    pub lsi: bool,
    pub voltage_scale: VoltageScale,
}

impl Default for Config {
    #[inline]
    fn default() -> Config {
        Config {
            mux: ClockSrc::MSI(MSIRange::default()),
            ahb_pre: AHBPrescaler::DIV1,
            apb1_pre: APBPrescaler::DIV1,
            apb2_pre: APBPrescaler::DIV1,
            #[cfg(crs)]
            enable_hsi48: false,
            rtc: None,
            lse: None,
            lsi: false,
            voltage_scale: VoltageScale::RANGE1,
        }
    }
}

pub(crate) unsafe fn init(config: Config) {
    // Set voltage scale
    while PWR.csr().read().vosf() {}
    PWR.cr().write(|w| w.set_vos(config.voltage_scale));
    while PWR.csr().read().vosf() {}

    let (sys_clk, sw) = match config.mux {
        ClockSrc::MSI(range) => {
            // Set MSI range
            RCC.icscr().write(|w| w.set_msirange(range.into()));

            // Enable MSI
            RCC.cr().write(|w| w.set_msion(true));
            while !RCC.cr().read().msirdy() {}

            let freq = 32_768 * (1 << (range as u8 + 1));
            (freq, Sw::MSI)
        }
        ClockSrc::HSI16 => {
            // Enable HSI16
            RCC.cr().write(|w| w.set_hsi16on(true));
            while !RCC.cr().read().hsi16rdyf() {}

            (HSI_FREQ.0, Sw::HSI16)
        }
        ClockSrc::HSE(freq) => {
            // Enable HSE
            RCC.cr().write(|w| w.set_hseon(true));
            while !RCC.cr().read().hserdy() {}

            (freq.0, Sw::HSE)
        }
        ClockSrc::PLL(src, mul, div) => {
            let freq = match src {
                PLLSource::HSE(freq) => {
                    // Enable HSE
                    RCC.cr().write(|w| w.set_hseon(true));
                    while !RCC.cr().read().hserdy() {}
                    freq.0
                }
                PLLSource::HSI16 => {
                    // Enable HSI
                    RCC.cr().write(|w| w.set_hsi16on(true));
                    while !RCC.cr().read().hsi16rdyf() {}
                    HSI_FREQ.0
                }
            };

            // Disable PLL
            RCC.cr().modify(|w| w.set_pllon(false));
            while RCC.cr().read().pllrdy() {}

            let freq = match mul {
                PLLMul::Mul3 => freq * 3,
                PLLMul::Mul4 => freq * 4,
                PLLMul::Mul6 => freq * 6,
                PLLMul::Mul8 => freq * 8,
                PLLMul::Mul12 => freq * 12,
                PLLMul::Mul16 => freq * 16,
                PLLMul::Mul24 => freq * 24,
                PLLMul::Mul32 => freq * 32,
                PLLMul::Mul48 => freq * 48,
            };

            let freq = match div {
                PLLDiv::Div2 => freq / 2,
                PLLDiv::Div3 => freq / 3,
                PLLDiv::Div4 => freq / 4,
            };
            assert!(freq <= 32_000_000);

            RCC.cfgr().write(move |w| {
                w.set_pllmul(mul.into());
                w.set_plldiv(div.into());
                w.set_pllsrc(src.into());
            });

            // Enable PLL
            RCC.cr().modify(|w| w.set_pllon(true));
            while !RCC.cr().read().pllrdy() {}

            (freq, Sw::PLL)
        }
    };

    BackupDomain::configure_ls(
        config.rtc.unwrap_or(RtcClockSource::NOCLOCK),
        config.lsi,
        config.lse.map(|_| Default::default()),
    );

    let wait_states = match config.voltage_scale {
        VoltageScale::RANGE1 => match sys_clk {
            ..=16_000_000 => 0,
            _ => 1,
        },
        VoltageScale::RANGE2 => match sys_clk {
            ..=8_000_000 => 0,
            _ => 1,
        },
        VoltageScale::RANGE3 => 0,
        _ => unreachable!(),
    };
    FLASH.acr().modify(|w| {
        w.set_latency(wait_states != 0);
    });

    RCC.cfgr().modify(|w| {
        w.set_sw(sw);
        w.set_hpre(config.ahb_pre.into());
        w.set_ppre1(config.apb1_pre.into());
        w.set_ppre2(config.apb2_pre.into());
    });

    let ahb_freq: u32 = match config.ahb_pre {
        AHBPrescaler::DIV1 => sys_clk,
        pre => {
            let pre: Hpre = pre.into();
            let pre = 1 << (pre.to_bits() as u32 - 7);
            sys_clk / pre
        }
    };

    let (apb1_freq, apb1_tim_freq) = match config.apb1_pre {
        APBPrescaler::DIV1 => (ahb_freq, ahb_freq),
        pre => {
            let pre: Ppre = pre.into();
            let pre: u8 = 1 << (pre.to_bits() - 3);
            let freq = ahb_freq / pre as u32;
            (freq, freq * 2)
        }
    };

    let (apb2_freq, apb2_tim_freq) = match config.apb2_pre {
        APBPrescaler::DIV1 => (ahb_freq, ahb_freq),
        pre => {
            let pre: Ppre = pre.into();
            let pre: u8 = 1 << (pre.to_bits() - 3);
            let freq = ahb_freq / pre as u32;
            (freq, freq * 2)
        }
    };

    #[cfg(crs)]
    if config.enable_hsi48 {
        // Reset CRS peripheral
        RCC.apb1rstr().modify(|w| w.set_crsrst(true));
        RCC.apb1rstr().modify(|w| w.set_crsrst(false));

        // Enable CRS peripheral
        RCC.apb1enr().modify(|w| w.set_crsen(true));

        // Initialize CRS
        CRS.cfgr().write(|w|

        // Select LSE as synchronization source
        w.set_syncsrc(crs::vals::Syncsrc::LSE));
        CRS.cr().modify(|w| {
            w.set_autotrimen(true);
            w.set_cen(true);
        });

        // Enable VREFINT reference for HSI48 oscillator
        SYSCFG.cfgr3().modify(|w| {
            w.set_enref_hsi48(true);
            w.set_en_vrefint(true);
        });

        // Select HSI48 as USB clock
        RCC.ccipr().modify(|w| w.set_hsi48msel(true));

        // Enable dedicated USB clock
        RCC.crrcr().modify(|w| w.set_hsi48on(true));
        while !RCC.crrcr().read().hsi48rdy() {}
    }

    set_freqs(Clocks {
        sys: Hertz(sys_clk),
        ahb1: Hertz(ahb_freq),
        apb1: Hertz(apb1_freq),
        apb2: Hertz(apb2_freq),
        apb1_tim: Hertz(apb1_tim_freq),
        apb2_tim: Hertz(apb2_tim_freq),
    });
}
