// SPDX-License-Identifier: GPL-3.0-or-later
//! macOS signals. None of these APIs needs a permission: `CGEventSourceSecondsSinceLastEventType`
//! reads the HID idle time, unlike an event tap, which needs Accessibility.

use std::ffi::c_void;
use std::time::Duration;

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};
use objc2_foundation::{
    ns_string, NSArray, NSCalendar, NSCalendarUnit, NSDate, NSDictionary, NSNumber, NSProcessInfo,
    NSProcessInfoThermalState, NSString,
};

use super::{IdleSource, Power, PowerSource, Thermal};

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> *mut AnyObject;
    fn IOPSCopyPowerSourcesList(blob: *const c_void) -> *mut NSArray<AnyObject>;
    fn IOPSGetPowerSourceDescription(
        blob: *const c_void,
        source: *const c_void,
    ) -> *const NSDictionary<NSString, AnyObject>;
}

pub struct MacPower;

impl PowerSource for MacPower {
    fn read(&self) -> Power {
        // The scheduler thread has no run loop to drain autoreleased objects.
        autoreleasepool(|_| {
            let info = NSProcessInfo::processInfo();
            let thermal = match info.thermalState() {
                NSProcessInfoThermalState::Critical => Thermal::Critical,
                NSProcessInfoThermalState::Serious => Thermal::Serious,
                NSProcessInfoThermalState::Fair => Thermal::Fair,
                _ => Thermal::Nominal,
            };
            let (on_battery, charge_percent) = internal_battery().unwrap_or((false, None));
            Power {
                on_battery,
                charge_percent,
                low_power_mode: info.isLowPowerModeEnabled(),
                thermal,
            }
        })
    }
}

/// (running on battery, charge percent) for the internal battery; None on a desktop Mac.
fn internal_battery() -> Option<(bool, Option<u8>)> {
    // SAFETY: both Copy functions return an owned (+1) CF object or null, and CF types are
    // toll-free bridged Objective-C objects, so `Retained` releases them correctly.
    let blob = unsafe { Retained::from_raw(IOPSCopyPowerSourcesInfo()) }?;
    let blob_ptr = Retained::as_ptr(&blob).cast::<c_void>();
    // SAFETY: as above; `blob` outlives the list.
    let list = unsafe { Retained::from_raw(IOPSCopyPowerSourcesList(blob_ptr)) }?;
    for source in list.iter() {
        let source_ptr = Retained::as_ptr(&source).cast::<c_void>();
        // SAFETY: a Get function: the dictionary is borrowed from `blob`, which is alive here.
        let Some(desc) = (unsafe { IOPSGetPowerSourceDescription(blob_ptr, source_ptr).as_ref() })
        else {
            continue;
        };
        let text = |k: &NSString| {
            desc.objectForKey(k)
                .and_then(|v| v.downcast::<NSString>().ok())
                .map(|s| s.to_string())
        };
        let number = |k: &NSString| {
            desc.objectForKey(k)
                .and_then(|v| v.downcast::<NSNumber>().ok())
                .map(|n| n.integerValue())
        };
        if text(ns_string!("Type")).as_deref() != Some("InternalBattery") {
            continue;
        }
        let on_battery = text(ns_string!("Power Source State")).as_deref() == Some("Battery Power");
        let charge = match (
            number(ns_string!("Current Capacity")),
            number(ns_string!("Max Capacity")),
        ) {
            (Some(cur), Some(max)) if max > 0 => u8::try_from((cur * 100 / max).clamp(0, 100)).ok(),
            _ => None,
        };
        return Some((on_battery, charge));
    }
    None
}

pub struct MacIdle;

impl IdleSource for MacIdle {
    fn idle(&self) -> Duration {
        // kCGAnyInputEventType is ~0.
        let secs = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType(u32::MAX),
        );
        Duration::try_from_secs_f64(secs).unwrap_or_default()
    }
}

pub fn local_minute_of_day() -> u16 {
    autoreleasepool(|_| {
        let cal = NSCalendar::currentCalendar();
        let now = NSDate::now();
        let h = cal.component_fromDate(NSCalendarUnit::Hour, &now);
        let m = cal.component_fromDate(NSCalendarUnit::Minute, &now);
        u16::try_from(h * 60 + m).unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the real machine; prints what it saw so a run on a laptop can be checked by eye.
    #[test]
    fn reads_this_mac() {
        let p = MacPower.read();
        let idle = MacIdle.idle();
        let minute = local_minute_of_day();
        eprintln!("power {p:?} idle {idle:?} minute {minute}");
        assert!(minute < 1440);
        assert!(idle < Duration::from_secs(60 * 60 * 24 * 365));
        if let Some(c) = p.charge_percent {
            assert!(c <= 100);
        }
    }
}
