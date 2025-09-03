use stm32_metapac as pac;

/// Setup the ADC for reading the analog channels
pub fn configure_adc() {
    pac::RCC.apbenr2().modify(|w| w.set_adcen(true));
    pac::ADC1.cr().modify(|w| {
        w.set_advregen(true);
    });

    // Delay 1/40th of a second for regulator to turn on
    cortex_m::asm::delay(16_000_000 / 40);

    pac::ADC1.cr().modify(|w| w.set_adcal(true));

    // Wait for calibration to complete
    while pac::ADC1.cr().read().adcal() {}

    // Clear ADRDY IRQ
    pac::ADC1.isr().write(|w| w.set_adrdy(true));
    // Enable
    pac::ADC1.cr().modify(|w| w.set_aden(true));

    // Wait for ADRDY signal
    while !pac::ADC1.isr().read().adrdy() {}
    // Clear the flag again
    pac::ADC1.isr().write(|w| w.set_adrdy(true));

    pac::ADC1.cfgr1().modify(|w| {
        w.set_cont(false);
    });

    // Enable oversampling
    pac::ADC1.cfgr2().modify(|w| {
        w.set_ovse(true);
        // 16x oversample
        w.set_ovsr(3);
        // shift by 4 bits
        w.set_ovss(4);
    });

    pac::ADC1
        .smpr()
        .modify(|w| w.set_smp1(pac::adc::vals::SampleTime::CYCLES39_5));
}

pub fn read_adc(channel: usize) -> u16 {
    // Configure channel
    pac::ADC1.chselr().write(|w| w.set_chsel(1 << channel));
    // Clear EOC
    pac::ADC1.isr().write(|w| w.set_eoc(true));
    // Start sampling
    pac::ADC1.cr().modify(|w| w.set_adstart(true));
    // Wait for complete
    while !pac::ADC1.isr().read().eoc() {}
    // Read result
    pac::ADC1.dr().read().regular_data()
}
