//! Dates, times, datetimes, months, and weekdays.

use std::cmp::{Ordering, PartialOrd};
use std::error::Error as ErrorTrait;
use std::fmt;
use std::ops::{Add, Sub};

use cal::{DatePiece, TimePiece};
use cal::fmt::ISO;
use cal::units::{Year, Month, Weekday};
use cal::units::Month::*;
use duration::Duration;
use instant::Instant;
use system::sys_time;
use util::RangeExt;


/// Number of days guaranteed to be in four years.
const DAYS_IN_4Y:   i64 = 365 *   4 +  1;

/// Number of days guaranteed to be in a hundred years.
const DAYS_IN_100Y: i64 = 365 * 100 + 24;

/// Number of days guaranteed to be in four hundred years.
const DAYS_IN_400Y: i64 = 365 * 400 + 97;

/// Number of seconds in a day. As everywhere in this library, leap seconds
/// are simply ignored.
const SECONDS_IN_DAY: i64 = 86400;


/// Number of days between  **1st January, 1970** and **1st March, 2000**.
///
/// This might seem like an odd number to calculate, instead of using the
/// 1st of January as a reference point, but it turs out that by having the
/// reference point immediately after a possible leap-year day, the maths
/// needed to calculate the day/week/month of an instant comes out a *lot*
/// simpler!
///
/// The Gregorian calendar operates on a 400-year cycle, so the combination
/// of having it on a year that’s a multiple of 400, and having the leap
/// day at the very end of one of these cycles, means that the calculations
/// are reduced to simple division (of course, with a bit of date-shifting
/// to base a date around this reference point).
///
/// Rust has the luxury of having been started *after* this date. In Win32,
/// the epoch is midnight, the 1st of January, 1601, for much the same
/// reasons - except that it was developed before the year 2000, so they
/// had to go all the way back to the *previous* 400-year multiple.[^win32]
///
/// The only problem is that many people assume the Unix epoch to be
/// midnight on the 1st January 1970, so this value (and any functions that
/// depend on it) aren’t exposed to users of this library.
///
/// [^win32]: http://blogs.msdn.com/b/oldnewthing/archive/2009/03/06/9461176.aspx
///
const EPOCH_DIFFERENCE: i64 = (30 * 365      // 30 years between 2000 and 1970...
                               + 7           // plus seven days for leap years...
                               + 31 + 29);   // plus all the days in January and February in 2000.


/// This rather strange triangle is an array of the number of days elapsed
/// at the end of each month, starting at the beginning of March (the first
/// month after the EPOCH above), going backwards, ignoring February.
const TIME_TRIANGLE: &'static [i64; 11] =
    &[31 + 30 + 31 + 30 + 31 + 31 + 30 + 31 + 30 + 31 + 31,  // January
      31 + 30 + 31 + 30 + 31 + 31 + 30 + 31 + 30 + 31,  // December
      31 + 30 + 31 + 30 + 31 + 31 + 30 + 31 + 30,  // November
      31 + 30 + 31 + 30 + 31 + 31 + 30 + 31,  // October
      31 + 30 + 31 + 30 + 31 + 31 + 30,  // September
      31 + 30 + 31 + 30 + 31 + 31,  // August
      31 + 30 + 31 + 30 + 31,  // July
      31 + 30 + 31 + 30,  // June
      31 + 30 + 31,  // May
      31 + 30,  // April
      31]; // March



/// A **local date** is a day-long span on the timeline, *without a time
/// zone*.
#[derive(Eq, Clone, Copy)]
pub struct Date {
    ymd:     YMD,
    yearday: i16,
    weekday: Weekday,
}

/// A **local time** is a time on the timeline that recurs once a day,
/// *without a time zone*.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Time {
    hour:   i8,
    minute: i8,
    second: i8,
    millisecond: i16,
}

/// A **local date-time** is an exact instant on the timeline, *without a
/// time zone*.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct DateTime {
    date: Date,
    time: Time,
}


impl Date {

    /// Creates a new local date instance from the given year, month, and day
    /// fields.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an error.
    ///
    /// ### Examples
    ///
    /// Instantiate the 20th of July 1969 based on its year,
    /// week-of-year, and weekday.
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::Month;
    /// use datetime::DatePiece;
    ///
    /// let date = Date::ymd(1969, Month::July, 20).unwrap();
    /// assert_eq!(date.year(), 1969);
    /// assert_eq!(date.month(), Month::July);
    /// assert_eq!(date.day(), 20);
    ///
    /// assert!(Date::ymd(2100, Month::February, 29).is_err());
    /// ```
    pub fn ymd(year: i64, month: Month, day: i8) -> Result<Date> {
        YMD { year: year, month: month, day: day }
            .to_days_since_epoch()
            .map(|days| Date::from_days_since_epoch(days - EPOCH_DIFFERENCE))
    }

    /// Creates a new local date instance from the given year and day-of-year
    /// values.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an error.
    ///
    /// ### Examples
    ///
    /// Instantiate the 13th of September 2015 based on its year
    /// and day-of-year.
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::{Weekday, Month, DatePiece};
    ///
    /// let date = Date::yd(2015, 0x100).unwrap();
    /// assert_eq!(date.year(), 2015);
    /// assert_eq!(date.month(), Month::September);
    /// assert_eq!(date.day(), 13);
    /// ```
    pub fn yd(year: i64, yearday: i64) -> Result<Date> {
        if yearday.is_within(0..367) {
            let jan_1 = YMD { year: year, month: January, day: 1 };
            let days = try!(jan_1.to_days_since_epoch());
            Ok(Date::from_days_since_epoch(days + yearday - 1 - EPOCH_DIFFERENCE))
        }
        else {
            Err(Error::OutOfRange)
        }
    }

    /// Creates a new local date instance from the given year, week-of-year,
    /// and weekday values.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an error.
    ///
    /// ### Examples
    ///
    /// Instantiate the 11th of September 2015 based on its year,
    /// week-of-year, and weekday.
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::{Weekday, Month, DatePiece};
    ///
    /// let date = Date::ywd(2015, 37, Weekday::Friday).unwrap();
    /// assert_eq!(date.year(), 2015);
    /// assert_eq!(date.month(), Month::September);
    /// assert_eq!(date.day(), 11);
    /// assert_eq!(date.weekday(), Weekday::Friday);
    /// ```
    ///
    /// Note that according to the ISO-8601 standard, the year will change
    /// when working with dates early in week 1, or late in week 53:
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::{Weekday, Month, DatePiece};
    ///
    /// let date = Date::ywd(2009, 1, Weekday::Monday).unwrap();
    /// assert_eq!(date.year(), 2008);
    /// assert_eq!(date.month(), Month::December);
    /// assert_eq!(date.day(), 29);
    /// assert_eq!(date.weekday(), Weekday::Monday);
    ///
    /// let date = Date::ywd(2009, 53, Weekday::Sunday).unwrap();
    /// assert_eq!(date.year(), 2010);
    /// assert_eq!(date.month(), Month::January);
    /// assert_eq!(date.day(), 3);
    /// assert_eq!(date.weekday(), Weekday::Sunday);
    /// ```
    pub fn ywd(year: i64, week: i64, weekday: Weekday) -> Result<Date> {
        let jan_4 = YMD { year: year, month: January, day: 4 };
        let correction = days_to_weekday(jan_4.to_days_since_epoch().unwrap() - EPOCH_DIFFERENCE)
            .days_from_monday_as_one() as i64 + 3;

        let yearday = 7 * week + weekday.days_from_monday_as_one() as i64 - correction;

        if yearday <= 0 {
            let days_in_year = if Year(year - 1).is_leap_year() { 366 } else { 365 };
            Date::yd(year - 1, days_in_year + yearday)
        }
        else {
            let days_in_year = if Year(year).is_leap_year() { 366 } else { 365 };

            if yearday >= days_in_year {
                Date::yd(year + 1, yearday - days_in_year)
            }
            else {
                Date::yd(year, yearday)
            }
        }
    }

    /// Computes a Date - year, month, day, weekday, and yearday -
    /// given the number of days that have passed since the EPOCH.
    ///
    /// This is used by all the other constructor functions.
    /// ### Examples
    ///
    /// Instantiate the 25th of September 2015 given its day-of-year (268).
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::{Month, DatePiece};
    ///
    /// let date = Date::yd(2015, 268).unwrap();
    /// assert_eq!(date.year(), 2015);
    /// assert_eq!(date.month(), Month::September);
    /// assert_eq!(date.day(), 25);
    /// ```
    ///
    /// Remember that on leap years, the number of days in a year changes:
    ///
    /// ```rust
    /// use datetime::local::Date;
    /// use datetime::{Month, DatePiece};
    ///
    /// let date = Date::yd(2016, 268).unwrap();
    /// assert_eq!(date.year(), 2016);
    /// assert_eq!(date.month(), Month::September);
    /// assert_eq!(date.day(), 24);  // not the 25th!
    /// ```
    fn from_days_since_epoch(days: i64) -> Date {

        // The Gregorian calendar works in 400-year cycles, which repeat
        // themselves ever after.
        //
        // This calculation works by finding the number of 400-year,
        // 100-year, and 4-year cycles, then constantly subtracting the
        // number of leftover days.
        let (num_400y_cycles, mut remainder) = split_cycles(days, DAYS_IN_400Y);

        // Calculate the numbers of 100-year cycles, 4-year cycles, and
        // leftover years, continually reducing the number of days left to
        // think about.
        let num_100y_cycles = remainder / DAYS_IN_100Y;
        remainder -= num_100y_cycles * DAYS_IN_100Y;  // remainder is now days left in this 100-year cycle

        let num_4y_cycles = remainder / DAYS_IN_4Y;
        remainder -= num_4y_cycles * DAYS_IN_4Y;  // remainder is now days left in this 4-year cycle

        let mut years = remainder / 365;
        remainder -= years * 365;  // remainder is now days left in this year

        // Leap year calculation goes thusly:
        //
        // 1. If the year is a multiple of 400, it’s a leap year.
        // 2. Else, if the year is a multiple of 100, it’s *not* a leap year.
        // 3. Else, if the year is a multiple of 4, it’s a leap year again!
        //
        // We already have the values for the numbers of multiples at this
        // point, and it’s safe to re-use them.
        let days_this_year =
            if years == 0 && !(num_4y_cycles == 0 && num_100y_cycles != 0) { 366 }
                                                                      else { 365 };

        // Find out which number day of the year it is.
        // The 306 here refers to the number of days in a year excluding
        // January and February (which are excluded because of the EPOCH)
        let mut day_of_year = remainder + days_this_year - 306;
        if day_of_year >= days_this_year {
            day_of_year -= days_this_year;  // wrap around for January and February
        }

        // Turn all those cycles into an actual number of years.
        years +=   4 * num_4y_cycles
               + 100 * num_100y_cycles
               + 400 * num_400y_cycles;

        // Work out the month and number of days into the month by scanning
        // the time triangle, finding the month that has the correct number
        // of days elapsed at the end of it.
        // (it’s “11 - index” below because the triangle goes backwards)
        let result = TIME_TRIANGLE.iter()
                                  .enumerate()
                                  .find(|&(_, days)| *days <= remainder);

        let (mut month, month_days) = match result {
            Some((index, days)) => (11 - index, remainder - *days),
            None => (0, remainder),  // No month found? Then it’s February.
        };

        // Need to add 2 to the month in order to compensate for the EPOCH
        // being in March.
        month += 2;

        if month >= 12 {
            years += 1;   // wrap around for January and February
            month -= 12;  // (yes, again)
        }

        // The check immediately above means we can `unwrap` this, as the
        // month number is guaranteed to be in the range (0..12).
        let month_variant = Month::from_zero(month as i8).unwrap();

        // Finally, adjust the day numbers for human reasons: the first day
        // of the month is the 1st, rather than the 0th, and the year needs
        // to be adjusted relative to the EPOCH.
        Date {
            yearday: (day_of_year + 1) as i16,
            weekday: days_to_weekday(days),
            ymd: YMD {
                year:  years + 2000,
                month: month_variant,
                day:   (month_days + 1) as i8,
            },
        }
    }

    /// Creates a new datestamp instance with the given year, month, day,
    /// weekday, and yearday fields.
    ///
    /// This function is unsafe because **the values are not checked for
    /// validity!** It’s possible to pass the wrong values in, such as having
    /// a wrong day value for a month, or having the yearday value out of
    /// step. Before using it, check that the values are all correct - or just
    /// use the `date!()` macro, which does this for you at compile-time.
    ///
    /// For this reason, the function is marked as `unsafe`, even though it
    /// (technically) uses unsafe components.
    pub unsafe fn _new_with_prefilled_values(year: i64, month: Month, day: i8, weekday: Weekday, yearday: i16) -> Date {
        Date {
            ymd: YMD { year: year, month: month, day: day },
            weekday: weekday,
            yearday: yearday,
        }
    }

    // I’m not 100% convinced on using `unsafe` for something that doesn’t
    // technically *need* to be unsafe, but I’ll stick with it for now.
}

impl DatePiece for Date {
    fn year(&self) -> i64 { self.ymd.year }
    fn month(&self) -> Month { self.ymd.month }
    fn day(&self) -> i8 { self.ymd.day }
    fn yearday(&self) -> i16 { self.yearday }
    fn weekday(&self) -> Weekday { self.weekday }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Date({})", self.iso())
    }
}

impl PartialEq for Date {
    fn eq(&self, other: &Date) -> bool {
        self.ymd == other.ymd
    }
}

impl PartialOrd for Date {
    fn partial_cmp(&self, other: &Date) -> Option<Ordering> {
        self.ymd.partial_cmp(&other.ymd)
    }
}

impl Ord for Date {
    fn cmp(&self, other: &Date) -> Ordering {
        self.ymd.cmp(&other.ymd)
    }
}

impl Time {

    /// Computes the number of hours, minutes, and seconds, based on the
    /// number of seconds that have elapsed since midnight.
    pub fn from_seconds_since_midnight(seconds: i64) -> Time {
        Time::from_seconds_and_milliseconds_since_midnight(seconds, 0)
    }

    /// Computes the number of hours, minutes, and seconds, based on the
    /// number of seconds that have elapsed since midnight.
    pub fn from_seconds_and_milliseconds_since_midnight(seconds: i64, millisecond_of_second: i16) -> Time {
        Time {
            hour:   (seconds / 60 / 60) as i8,
            minute: (seconds / 60 % 60) as i8,
            second: (seconds % 60) as i8,
            millisecond: millisecond_of_second,
        }
    }

    /// Returns the time at midnight, with all fields initialised to 0.
    pub fn midnight() -> Time {
        Time { hour: 0, minute: 0, second: 0, millisecond: 0 }
    }

    /// Creates a new timestamp instance with the given hour and minute
    /// fields. The second and millisecond fields are set to 0.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an `Err`.
    pub fn hm(hour: i8, minute: i8) -> Result<Time> {
        if (hour.is_within(0..24) && minute.is_within(0..60))
        || (hour == 24 && minute == 00) {
            Ok(Time { hour: hour, minute: minute, second: 0, millisecond: 0 })
        }
        else {
            Err(Error::OutOfRange)
        }
    }

    /// Creates a new timestamp instance with the given hour, minute, and
    /// second fields. The millisecond field is set to 0.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an `Err`.
    pub fn hms(hour: i8, minute: i8, second: i8) -> Result<Time> {
        if (hour.is_within(0..24) && minute.is_within(0..60) && second.is_within(0..60))
        || (hour == 24 && minute == 00 && second == 00) {
            Ok(Time { hour: hour, minute: minute, second: second, millisecond: 0 })
        }
        else {
            Err(Error::OutOfRange)
        }
    }

    /// Creates a new timestamp instance with the given hour, minute,
    /// second, and millisecond fields.
    ///
    /// The values are checked for validity before instantiation, and
    /// passing in values out of range will return an `Err`.
    pub fn hms_ms(hour: i8, minute: i8, second: i8, millisecond: i16) -> Result<Time> {
        if hour.is_within(0..24)   && minute.is_within(0..60)
        && second.is_within(0..60) && millisecond.is_within(0..1000)
        {
            Ok(Time { hour: hour, minute: minute, second: second, millisecond: millisecond })
        }
        else {
            Err(Error::OutOfRange)
        }
    }

    /// Calculate the number of seconds since midnight this time is at,
    /// ignoring milliseconds.
    pub fn to_seconds(&self) -> i64 {
        self.hour as i64 * 3600
            + self.minute as i64 * 60
            + self.second as i64
    }
}

impl TimePiece for Time {
    fn hour(&self) -> i8 { self.hour }
    fn minute(&self) -> i8 { self.minute }
    fn second(&self) -> i8 { self.second }
    fn millisecond(&self) -> i16 { self.millisecond }
}

impl fmt::Debug for Time {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Time({})", self.iso())
    }
}


impl DateTime {

    /// Computes a complete date-time based on the values in the given
    /// Instant parameter.
    pub fn from_instant(instant: Instant) -> DateTime {
        DateTime::at_ms(instant.seconds(), instant.milliseconds())
    }

    /// Computes a complete date-time based on the number of seconds that
    /// have elapsed since **midnight, 1st January, 1970**, setting the
    /// number of milliseconds to 0.
    pub fn at(seconds_since_1970_epoch: i64) -> DateTime {
        DateTime::at_ms(seconds_since_1970_epoch, 0)
    }

    /// Computes a complete date-time based on the number of seconds that
    /// have elapsed since **midnight, 1st January, 1970**,
    pub fn at_ms(seconds_since_1970_epoch: i64, millisecond_of_second: i16) -> DateTime {
        let seconds = seconds_since_1970_epoch - EPOCH_DIFFERENCE * SECONDS_IN_DAY;

        // Just split the input value into days and seconds, and let
        // Date and Time do all the hard work.
        let (days, secs) = split_cycles(seconds, SECONDS_IN_DAY);

        DateTime {
            date: Date::from_days_since_epoch(days),
            time: Time::from_seconds_and_milliseconds_since_midnight(secs, millisecond_of_second),
        }
    }

    /// Creates a new local date time from a local date and a local time.
    pub fn new(date: Date, time: Time) -> DateTime {
        DateTime {
            date: date,
            time: time,
        }
    }

    /// Returns the date portion of this date-time stamp.
    pub fn date(&self) -> Date {
        self.date
    }

    /// Returns the time portion of this date-time stamp.
    pub fn time(&self) -> Time {
        self.time
    }

    /// Creates a new date-time stamp set to the current time.
    pub fn now() -> DateTime {
        let (s, ms) = unsafe { sys_time() };
        DateTime::at_ms(s, ms)
    }

    pub fn to_instant(&self) -> Instant {
        let seconds = self.date.ymd.to_days_since_epoch().unwrap() * SECONDS_IN_DAY
            + self.time.to_seconds();

        Instant::at_ms(seconds, self.time.millisecond)
    }

    pub fn add_seconds(&self, seconds: i64) -> DateTime {
        Self::from_instant(self.to_instant() + Duration::of(seconds))
    }
}

impl DatePiece for DateTime {
    fn year(&self) -> i64 { self.date.ymd.year }
    fn month(&self) -> Month { self.date.ymd.month }
    fn day(&self) -> i8 { self.date.ymd.day }
    fn yearday(&self) -> i16 { self.date.yearday }
    fn weekday(&self) -> Weekday { self.date.weekday }
}

impl TimePiece for DateTime {
    fn hour(&self) -> i8 { self.time.hour }
    fn minute(&self) -> i8 { self.time.minute }
    fn second(&self) -> i8 { self.time.second }
    fn millisecond(&self) -> i16 { self.time.millisecond }
}

impl fmt::Debug for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DateTime({})", self.iso())
    }
}

impl Add<Duration> for DateTime {
    type Output = DateTime;

    fn add(self, duration: Duration) -> DateTime {
        DateTime::from_instant(self.to_instant() + duration)
    }
}

impl Sub<Duration> for DateTime {
    type Output = DateTime;

    fn sub(self, duration: Duration) -> DateTime {
        DateTime::from_instant(self.to_instant() - duration)
    }
}


/// A **YMD** is an implementation detail of Date. It provides
/// helper methods relating to the construction of Date instances.
///
/// The main difference is that while all Dates get checked for
/// validity before they are used, there is no such check for YMD. The
/// interface to Date ensures that it should be impossible to
/// create an instance of the 74th of March, for example, but you’re
/// free to create such an instance of YMD. For this reason, it is not
/// exposed to implementors of this library.
#[derive(PartialEq, PartialOrd, Eq, Ord, Clone, Debug, Copy)]
struct YMD {
    year:    i64,
    month:   Month,
    day:     i8,
}

impl YMD {

    /// Calculates the number of days that have elapsed since the 1st
    /// January, 1970. Returns the number of days if this datestamp is
    /// valid; None otherwise.
    ///
    /// This method returns a Result instead of exposing is_valid to
    /// the user, because the leap year calculations are used in both
    /// functions, so it makes more sense to only do them once.
    pub fn to_days_since_epoch(&self) -> Result<i64> {
        let years = self.year - 2000;
        let (leap_days_elapsed, is_leap_year) = Year(self.year).leap_year_calculations();

        if !self.is_valid(is_leap_year) {
            return Err(Error::OutOfRange);
        }

        // Work out the number of days from the start of 1970 to now,
        // which is a multiple of the number of years...
        let days = years * 365

            // Plus the number of days between the start of 2000 and the
            // start of 1970, to make up the difference because our
            // dates start at 2000 and instants start at 1970...
            + 10958

            // Plus the number of leap years that have elapsed between
            // now and the start of 2000...
            + leap_days_elapsed

            // Plus the number of days in all the months leading up to
            // the current month...
            + self.month.days_before_start() as i64

            // Plus an extra leap day for *this* year...
            + if is_leap_year && self.month >= March { 1 } else { 0 }

            // Plus the number of days in the month so far! (Days are
            // 1-indexed, so we make them 0-indexed here)
            + (self.day - 1) as i64;

        Ok(days)
    }

    /// Returns whether this datestamp is valid, which basically means
    /// whether the day is in the range allowed by the month.
    ///
    /// Whether the current year is a leap year should already have been
    /// calculated at this point, so the value is passed in rather than
    /// calculating it afresh.
    pub fn is_valid(&self, is_leap_year: bool) -> bool {
        self.day >= 1 && self.day <= self.month.days_in_month(is_leap_year)
    }
}

/// Computes the weekday, given the number of days that have passed
/// since the EPOCH.
fn days_to_weekday(days: i64) -> Weekday {
    // March 1st, 2000 was a Wednesday, so add 3 to the number of days.
    let weekday = (days + 3) % 7;

    // We can unwrap since we’ve already done the bounds checking.
    Weekday::from_zero(if weekday < 0 { weekday + 7 } else { weekday } as i8).unwrap()
}

/// Split a number of years into a number of year-cycles, and the number
/// of years left over that don’t fit into a cycle. This is also used
/// for day-cycles.
///
/// This is essentially a division operation with the result and the
/// remainder, with the difference that a negative value gets ‘wrapped
/// around’ to be a positive value, owing to the way the modulo operator
/// works for negative values.
pub fn split_cycles(number_of_periods: i64, cycle_length: i64) -> (i64, i64) {
    let mut cycles    = number_of_periods / cycle_length;
    let mut remainder = number_of_periods % cycle_length;

    if remainder < 0 {
        remainder += cycle_length;
        cycles    -= 1;
    }

    (cycles, remainder)
}


#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Error {
    OutOfRange,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl ErrorTrait for Error {
    fn description(&self) -> &str {
        "datetime field out of range"
    }
}

use std::result;
pub type Result<T> = result::Result<T, Error>;


/// Misc tests that don’t seem to fit anywhere.
#[cfg(test)]
mod test {
    pub use super::{DateTime, Date, Time};
    pub use cal::units::{Month, Weekday, Year};
    pub use cal::DatePiece;
    pub use std::str::FromStr;


    #[test]
    fn some_leap_years() {
        for year in [2004,2008,2012,2016].iter() {
            assert!(Date::ymd(*year, Month::February, 29).is_ok());
            assert!(Date::ymd(*year + 1, Month::February, 29).is_err());
        }
        assert!(Date::ymd(1600,Month::February,29).is_ok());
        assert!(Date::ymd(1601,Month::February,29).is_err());
        assert!(Date::ymd(1602,Month::February,29).is_err());
    }

    #[test]
    fn new() {
        for year in 1..3000 {
            assert!(Date::ymd(year, Month::January,   32).is_err());
            assert!(Date::ymd(year, Month::February,  30).is_err());
            assert!(Date::ymd(year, Month::March,     32).is_err());
            assert!(Date::ymd(year, Month::April,     31).is_err());
            assert!(Date::ymd(year, Month::May,       32).is_err());
            assert!(Date::ymd(year, Month::June,      31).is_err());
            assert!(Date::ymd(year, Month::July,      32).is_err());
            assert!(Date::ymd(year, Month::August,    32).is_err());
            assert!(Date::ymd(year, Month::September, 31).is_err());
            assert!(Date::ymd(year, Month::October,   32).is_err());
            assert!(Date::ymd(year, Month::November,  31).is_err());
            assert!(Date::ymd(year, Month::December,  32).is_err());
        }
    }

    #[test]
    fn to_from_days_since_epoch() {
        let epoch_difference: i64 = 30 * 365 + 7 + 31 + 29;  // see EPOCH_DIFFERENCE
        for date in  vec![
            Date::ymd(1970, Month::January,   1).unwrap(),
            Date::ymd(  01, Month::January,   1).unwrap(),
            Date::ymd(1971, Month::January,   1).unwrap(),
            Date::ymd(1973, Month::January,   1).unwrap(),
            Date::ymd(1977, Month::January,   1).unwrap(),
            Date::ymd(1989, Month::November, 10).unwrap(),
            Date::ymd(1990, Month::July,      8).unwrap(),
            Date::ymd(2014, Month::July,     13).unwrap(),
            Date::ymd(2001, Month::February,  3).unwrap()
        ]{
            assert_eq!( date,
                Date::from_days_since_epoch(
                    date.ymd.to_days_since_epoch().unwrap() - epoch_difference));
        }
    }

    mod debug {
        use super::*;

        #[test]
        fn recently() {
            let date = Date::ymd(1600, Month::February, 28).unwrap();
            let debugged = format!("{:?}", date);

            assert_eq!(debugged, "Date(1600-02-28)");
        }

        #[test]
        fn just_then() {
            let date = Date::ymd(-753, Month::December, 1).unwrap();
            let debugged = format!("{:?}", date);

            assert_eq!(debugged, "Date(-0753-12-01)");
        }

        #[test]
        fn far_far_future() {
            let date = Date::ymd(10601, Month::January, 31).unwrap();
            let debugged = format!("{:?}", date);

            assert_eq!(debugged, "Date(+10601-01-31)");
        }

        #[test]
        fn midday() {
            let time = Time::hms(12, 0, 0).unwrap();
            let debugged = format!("{:?}", time);

            assert_eq!(debugged, "Time(12:00:00.000)");
        }

        #[test]
        fn ascending() {
            let then = DateTime::new(
                        Date::ymd(2009, Month::February, 13).unwrap(),
                        Time::hms(23, 31, 30).unwrap());
            let debugged = format!("{:?}", then);

            assert_eq!(debugged, "DateTime(2009-02-13T23:31:30.000)");
        }
    }
}