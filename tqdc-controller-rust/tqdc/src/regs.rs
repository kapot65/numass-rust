use strum::IntoEnumIterator;
use strum_macros::EnumIter;

const RUN_LOGIC: isize = 0x0060;

#[derive(EnumIter, Debug, Clone, Copy)]
pub enum Register16 {
    // TODO: implement registers in rust enum way
    DeviceId = 0x0042,
    DeviceCtrl = 0x0040,
    RunState = 0x0061,
    TriggerCSR = 0x0100,
    SelfTriggerDelay = 0x0108,
    SelfTriggerMask = 0x107,
    AdcChannelEnabledMask = 0x304,
    AdcGainControlMask = 0x306,
    AdcMode = 0x305,
    Dac1Ctrl = 0x4200,
    Dac2Ctrl = 0x4201,
    
    
    // new registes
    RunLogicControl = RUN_LOGIC + 0x02,
    RunLogicRunMode = RUN_LOGIC + 0x04,
    
    RunLogicRunState = RUN_LOGIC + 0x05 // TODO: remove runstate
    
}

#[derive(Debug, Clone, Copy)]
pub enum RunLogicControl {
    Run = 0x8000,
    SoftClear = 0x0001,
    Stop = 0x0000, // TODO: better name?
}


#[derive(EnumIter, Debug, Clone, Copy)]
pub enum Register32 {
    TimeLimit = 0x0068, // time limit in mulliseconds
    RunMode = 0x0060,
    TriggerTimerPeriod = 0x0102,
    TriggerEventNumLoad = 0x0104,
    HitCountCh16 = 0x069E,
    TdcChannelEnable1 = 0x200,
    TdcChannelEnable2 = 0x202,
    TdcTrigWinSetup = 0x226,
    
    
    // New registers
    RunTimeLimit = RUN_LOGIC + 0x06,
    TriggerInfoLocalEventNumberLoadValue2 = 0x0a00 + 0x11,
    
}

#[derive(Debug, Clone, Copy)]
pub enum AdcMode {
    Norm = 0x0001,
    Baseline = 0x0002,
}

#[derive(Debug, Clone, Copy)]
pub enum RunMode {
    Time = 0x0001,
    Frames = 0x0000,
}

#[derive(EnumIter, Debug, Clone, Copy)]
pub enum RunState {
    Finished = 0x0002,
    InRun = 0x0001,
    Stopped = 0x0000,
}

#[derive(Debug, Clone, Copy)]
pub enum DeviceCtrl {
    Run = 0x8000,
    Stop = 0x0000,
}

#[derive(Debug, Clone, Copy)]
pub enum TriggerCSR {
    Exec = 0x0000, // ? find usages
    CountReset = 0x0001,
    RunCountReset = 0x0002,
    CountLock = 0x0004,
}

pub fn as_run_state(value: u16) -> Option<RunState> {
    RunState::iter().find(|reg| value == *reg as u16)
}

pub fn is_reg16(address: u16) -> Option<Register16> {
    Register16::iter().find(|reg| address == *reg as u16)
}

pub fn is_reg32(address: u16) -> Option<Register32> {
    Register32::iter().find(|reg| address == *reg as u16)
}
