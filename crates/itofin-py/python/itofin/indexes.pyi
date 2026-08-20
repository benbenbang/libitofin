# Hand-written stubs for itofin.indexes; sync manually with src/hullwhite.rs, src/helpers.rs,
# src/swapindex.rs, src/currency.rs and src/inflation.rs (#517).

from itofin import Settings
from itofin.termstructures import (
    YieldTermStructure,
    YoYInflationTermStructure,
    ZeroInflationTermStructure,
)
from itofin.time import (
    BusinessDayConvention,
    Calendar,
    Date,
    DayCounter,
    Period,
)

class Currency:
    """An ISO 4217 currency specification.

    Only the three named currencies the core provides are exposed; the general
    constructor is omitted, as the core ports only the currencies its indexes
    need and the full catalogue is deferred there.
    """

    @staticmethod
    def eur() -> Currency:
        """Return the European Euro.

        Returns:
            Currency: The euro, ISO code "EUR".
        """
        ...
    @staticmethod
    def usd() -> Currency:
        """Return the U.S. dollar.

        Returns:
            Currency: The U.S. dollar, ISO code "USD".
        """
        ...
    @staticmethod
    def gbp() -> Currency:
        """Return the British pound sterling.

        Returns:
            Currency: The pound sterling, ISO code "GBP".
        """
        ...
    def code(self) -> str:
        """Return the ISO 4217 three-letter code.

        Returns:
            str: The three-letter code, e.g. "EUR".
        """
        ...
    def __repr__(self) -> str:
        """Return the printable representation, which prints the ISO code.

        Returns:
            str: A string of the form Currency(EUR).
        """
        ...

class IborIndex:
    """A general Inter-Bank-Offered-Rate index, spelling out every convention.

    The form for an index outside the named families (the USD-3M IsdaIbor the
    ISDA CDS curve bootstraps off, say). Pass forwarding=None to build it over
    an empty handle, the form the bootstrap rate helpers need.

    It is the base of Euribor, and every Ibor-index consumer takes this type and
    accepts either: the deposit, swap, FRA and futures rate helpers, and the
    swap, swap-index, optionlet-volatility, cap/floor and swaption-helper
    facades. The OIS helper is not one of them; it takes the overnight Estr,
    which is not an IborIndex.
    """

    def __init__(
        self,
        family_name: str,
        tenor: Period,
        settlement_days: int,
        currency: Currency,
        fixing_calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        forwarding: YieldTermStructure | None,
        settings: Settings,
    ) -> None:
        """Build an index spelling out every convention the core constructor takes.

        The index fixes settlement_days before its value date on the fixing
        calendar, rolls to maturity under convention and end_of_month, accrues
        on day_counter and forecasts off forwarding.

        Args:
            family_name (str): The index family the fixings are stored under.
            tenor (Period): The index tenor, normalized at construction.
            settlement_days (int): The business days between the fixing date and
                the value date.
            currency (Currency): The currency the index is quoted in.
            fixing_calendar (Calendar): The calendar the fixing and value dates
                roll on.
            convention (BusinessDayConvention): The convention applied when
                rolling the value date to maturity.
            end_of_month (bool): Whether the maturity roll keeps to month ends.
            day_counter (DayCounter): The day count the index accrues on.
            forwarding (YieldTermStructure | None): The curve fixings are
                forecast off; None builds the index over an empty handle, the
                form the bootstrap rate helpers need.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool) -> float:
        """Return the index fixing for fixing_date.

        Forecast off the forwarding curve for a future date, or read from the
        stored fixings for a past one.

        Args:
            fixing_date (Date): The date the fixing is read or forecast for.
            forecast_todays_fixing (bool): Whether a fixing dated today is
                forecast rather than looked up.

        Returns:
            float: The fixing rate.

        Raises:
            ItofinError: If the forwarding handle is empty or the evaluation
                date is unset.
        """
        ...
    def value_date(self, fixing_date: Date) -> Date:
        """Return the value date of the loan fixed on fixing_date.

        The fixing date moved forward by the index's fixing days on the fixing
        calendar.

        Args:
            fixing_date (Date): The fixing date to advance.

        Returns:
            Date: The value date.

        Raises:
            ItofinError: If fixing_date is not a business day on the fixing
                calendar.
        """
        ...
    def fixing_date(self, value_date: Date) -> Date:
        """Return the fixing date of the loan starting on value_date.

        The value date moved back by the index's fixing days, the inverse of
        value_date.

        Args:
            value_date (Date): The value date to step back from.

        Returns:
            Date: The fixing date.
        """
        ...
    def maturity_date(self, value_date: Date) -> Date:
        """Return the maturity of the loan starting on value_date.

        The value date rolled on by the index tenor under the index's own
        convention and end-of-month flag.

        Args:
            value_date (Date): The date the loan starts on.

        Returns:
            Date: The maturity date.

        Raises:
            ItofinError: If the core rejects the roll.
        """
        ...
    def tenor(self) -> Period:
        """Return the index tenor, normalized at construction.

        Returns:
            Period: The index tenor.
        """
        ...
    def day_counter(self) -> DayCounter:
        """Return the day counter the index accrues on.

        Returns:
            DayCounter: The index day count.
        """
        ...
    def fixing_calendar(self) -> Calendar:
        """Return the calendar the fixing and value dates roll on.

        Returns:
            Calendar: The fixing calendar.
        """
        ...
    def business_day_convention(self) -> BusinessDayConvention:
        """Return the convention applied when rolling the value date to maturity.

        Returns:
            BusinessDayConvention: The stored convention.
        """
        ...
    def end_of_month(self) -> bool:
        """Return whether the maturity roll keeps to month ends.

        Returns:
            bool: True if the roll is end-of-month.
        """
        ...

class Euribor(IborIndex):
    """The Euribor IBOR index family.

    A subclass of IborIndex, so a Euribor is accepted wherever the general index
    is. It retains its own clone of the index the base holds - the same object,
    not a rebuild - so its own fixing reads exactly what the base reads.
    """

    def __init__(
        self, tenor: Period, curve: YieldTermStructure | None, settings: Settings
    ) -> None:
        """Build a Euribor index of the given tenor.

        Args:
            tenor (Period): The index tenor.
            curve (YieldTermStructure | None): The forwarding curve; None builds
                the index over an empty handle, the form the bootstrap rate
                helpers need.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Raises:
            ItofinError: If tenor is a daily tenor, which needs the dedicated
                daily-tenor constructor the core keeps separate.
        """
        ...
    @staticmethod
    def three_months(curve: YieldTermStructure, settings: Settings) -> Euribor:
        """Return the 3-month Euribor index forwarding off curve.

        Args:
            curve (YieldTermStructure): The forwarding curve.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            Euribor: The Euribor3M index.
        """
        ...
    @staticmethod
    def six_months(curve: YieldTermStructure, settings: Settings) -> Euribor:
        """Return the 6-month Euribor index forwarding off curve.

        Args:
            curve (YieldTermStructure): The forwarding curve.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            Euribor: The Euribor6M index.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool) -> float:
        """Return the index fixing for fixing_date.

        Forecast off the forwarding curve for a future date, or read from the
        stored fixings for a past one.

        Args:
            fixing_date (Date): The date the fixing is read or forecast for.
            forecast_todays_fixing (bool): Whether a fixing dated today is
                forecast rather than looked up.

        Returns:
            float: The fixing rate.

        Raises:
            ItofinError: If the forwarding handle is empty or the evaluation
                date is unset.
        """
        ...

class OvernightIndex:
    """The base of the overnight index families.

    Abstract: it has no constructor, because the core builds an overnight index
    only through a family factory such as Estr. It exists so OISRateHelper and
    MakeOis name one type and accept any family. The fixing accessor stays on
    the family facade; lifting it here is deferred.
    """

class Estr(OvernightIndex):
    """The Euro Short-Term Rate overnight index.

    A subclass of OvernightIndex, so an ESTR index is accepted wherever the
    general overnight index is. It retains its own clone of the index the base
    holds - the same object, not a rebuild - so a facade typed on either half
    reads exactly the same core index.
    """

    def __init__(self, curve: YieldTermStructure | None, settings: Settings) -> None:
        """Build an ESTR index forwarding off curve.

        Infallible, unlike the Euribor constructor: the overnight tenor is fixed
        to one day by the base rather than taken from the caller.

        Args:
            curve (YieldTermStructure | None): The forwarding curve; None builds
                the index over an empty forwarding handle, the form the OIS
                bootstrap needs.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool) -> float:
        """Return the index fixing for fixing_date.

        Forecast off the forwarding curve for a future date, or read from the
        stored fixings for a past one.

        Args:
            fixing_date (Date): The date the fixing is read or forecast for.
            forecast_todays_fixing (bool): Whether a fixing dated today is
                forecast rather than looked up.

        Returns:
            float: The fixing rate.

        Raises:
            ItofinError: If the forwarding handle is empty or the evaluation
                date is unset.
        """
        ...

class SwapIndex:
    """The index whose fixing is the fair rate of an on-the-fly vanilla swap,
    assembled from the index tenor, the forecasting Ibor index and the fixed-leg
    conventions.

    The swap is assembled off the value date the fixing date implies. The
    swaption volatility cubes take two of these (a long and a short base) and
    read the at-the-money forward off them, so this is the index the cube
    facades stack on rather than one priced with directly.

    The currency is inert for every ported consumer, so currency() reading it
    back off the core index is the only place it shows. Deferred (visible): the
    clone family (re-curving / re-tenoring) is deferred in the core itself.
    """

    def __init__(
        self,
        family_name: str,
        tenor: Period,
        settlement_days: int,
        currency: Currency,
        calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: IborIndex,
        settings: Settings,
    ) -> None:
        """Build a swap index forecasting and discounting off one curve.

        Both legs use the ibor index's forwarding curve. The index registers
        with that index, so a relinked curve notifies observers.

        Args:
            family_name (str): The index family the fixings are stored under.
            tenor (Period): The tenor of the underlying swap.
            settlement_days (int): The business days between the fixing date and
                the swap's start.
            currency (Currency): The index currency, inert for every ported
                consumer and read back only by currency().
            calendar (Calendar): The calendar the swap's dates roll on.
            fixed_leg_tenor (Period): The fixed leg's payment tenor.
            fixed_leg_convention (BusinessDayConvention): The fixed leg's
                business-day convention.
            fixed_leg_day_counter (DayCounter): The fixed leg's day count.
            ibor_index (IborIndex): The index forecasting the floating leg,
                whose forwarding curve also discounts.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
        """
        ...
    @staticmethod
    def with_exogenous_discount(
        family_name: str,
        tenor: Period,
        settlement_days: int,
        currency: Currency,
        calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: IborIndex,
        discount: YieldTermStructure,
        settings: Settings,
    ) -> SwapIndex:
        """Build a swap index discounting off a separate curve.

        The floating leg is still forecast off the ibor index's forwarding
        curve, but discounting uses discount. The index registers with both.

        Args:
            family_name (str): The index family the fixings are stored under.
            tenor (Period): The tenor of the underlying swap.
            settlement_days (int): The business days between the fixing date and
                the swap's start.
            currency (Currency): The index currency, inert for every ported
                consumer and read back only by currency().
            calendar (Calendar): The calendar the swap's dates roll on.
            fixed_leg_tenor (Period): The fixed leg's payment tenor.
            fixed_leg_convention (BusinessDayConvention): The fixed leg's
                business-day convention.
            fixed_leg_day_counter (DayCounter): The fixed leg's day count.
            ibor_index (IborIndex): The index forecasting the floating leg.
            discount (YieldTermStructure): The separate curve both legs are
                discounted on.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            SwapIndex: The index discounting off the exogenous curve.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool = False) -> float:
        """Return the underlying swap's fair rate for fixing_date.

        This is the at-the-money forward the volatility cubes read.

        Args:
            fixing_date (Date): The date the underlying swap is struck off.
            forecast_todays_fixing (bool): Whether a fixing dated today is
                forecast rather than looked up.

        Returns:
            float: The fair rate of the underlying swap.

        Raises:
            ItofinError: If the forwarding handle is empty, the evaluation date
                is unset, or the fixing date is invalid.
        """
        ...
    def currency(self) -> Currency:
        """Return the index currency, read back off the core index.

        Returns:
            Currency: The currency the index was built with.
        """
        ...
    def fixed_leg_tenor(self) -> Period:
        """Return the fixed leg's payment tenor.

        Returns:
            Period: The fixed-leg tenor.
        """
        ...
    def exogenous_discount(self) -> bool:
        """Return whether the index discounts off a separate curve.

        Returns:
            bool: True if the index was built by with_exogenous_discount.
        """
        ...

class CpiInterpolationType:
    """How a CPI observation interpolates between the index fixings bracketing it.

    Flat reads the fixing of the lagged period outright; Linear advances from it
    to the next period's fixing by how far the observation date has run into its
    own period. The core's deprecated AsIndex variant is not ported and so has
    no counterpart here.
    """

    Flat: CpiInterpolationType
    Linear: CpiInterpolationType

class ZeroInflationIndex:
    """A price index publishing one level per period, reading back either a
    stored figure or a forecast off its inflation curve.

    The curve is reached through a relinkable handle the index owns, so an
    index can be built before the curve it forecasts off exists. The handle
    starts empty and a forecast before any link raises ItofinError; link_to
    fills it.
    """

    @staticmethod
    def uk_rpi(settings: Settings) -> ZeroInflationIndex:
        """Return the UK Retail Price Index: monthly, one-month availability lag.

        Args:
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            ZeroInflationIndex: The "UK RPI" index, over an empty curve handle.
        """
        ...
    @staticmethod
    def uk_hicp(settings: Settings) -> ZeroInflationIndex:
        """Return the UK harmonised index of consumer prices.

        Args:
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            ZeroInflationIndex: The UK HICP index, over an empty curve handle.
        """
        ...
    @staticmethod
    def eu_hicp(settings: Settings) -> ZeroInflationIndex:
        """Return the euro-area harmonised index of consumer prices.

        Args:
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.

        Returns:
            ZeroInflationIndex: The EU HICP index, over an empty curve handle.
        """
        ...
    def name(self) -> str:
        """Return the index name, under which fixings are stored.

        Returns:
            str: The name, e.g. "UK RPI".
        """
        ...
    def add_fixing(self, fixing_date: Date, value: float) -> None:
        """Record a published figure across the whole inflation period.

        The figure is stored on every date of the period fixing_date falls in,
        so a later read on any day inside that period finds it.

        Args:
            fixing_date (Date): Any date inside the inflation period the figure
                describes.
            value (float): The published index level.

        Raises:
            ItofinError: If the index frequency has no expressible inflation
                period, or a different figure is already stored on a date in
                that period.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool = False) -> float:
        """Return the fixing at fixing_date, stored or forecast off the linked curve.

        Args:
            fixing_date (Date): The date the level is read or forecast for.
            forecast_todays_fixing (bool): Accepted and ignored, as in the core:
                needs_forecast alone decides between history and forecast.

        Returns:
            float: The index level.

        Raises:
            ItofinError: If a date the store should cover has no figure, or a
                forecast is asked for with no curve linked.
        """
        ...
    def last_fixing_date(self) -> Date:
        """Return the first day of the period the latest stored figure describes.

        Returns:
            Date: The start of that inflation period.

        Raises:
            ItofinError: If the index has no fixing history.
        """
        ...
    def link_to(self, curve: ZeroInflationTermStructure) -> None:
        """Point the index at curve, so every forecast from here on compounds off it.

        Takes the ZeroInflationTermStructure base, so any subclass links. It is
        the curve behind that facade's handle at call time that is stored, not
        the handle itself: relinking the facade afterwards leaves this index on
        the curve it was given, and a later link_to is how it moves.

        Args:
            curve (ZeroInflationTermStructure): The curve forecasts compound
                off.

        Raises:
            ItofinError: If curve somehow carries no link.
        """
        ...
    def needs_forecast(self, fixing_date: Date) -> bool:
        """Return whether fixing_date has to be forecast rather than read from history.

        Decided against the latest period that could have been published by the
        settings' evaluation date.

        Args:
            fixing_date (Date): The date in question.

        Returns:
            bool: True if the date has to be forecast off the curve.

        Raises:
            ItofinError: If the evaluation date is unset, or the index frequency
                has no expressible inflation period.
        """
        ...
    def __repr__(self) -> str:
        """Return the printable representation.

        Returns:
            str: A string of the form ZeroInflationIndex(UK RPI).
        """
        ...

class YoYInflationIndex:
    """An index publishing one year-on-year inflation rate per period, read
    back as a stored figure or forecast off its year-on-year curve.

    Two forms. A ratio index (from_underlying) derives its rate from two
    ZeroInflationIndex fixings a year apart and owns no history of its own; a
    quoted one (the constructor) is published as a rate in its own right and
    keeps its own history through add_fixing.

    Both forms link to a relinkable handle the index owns, so an index can be
    built before the curve it forecasts off exists. The handle starts empty and
    a forecast before any link raises ItofinError; link_to fills it.

    The quoted constructor spells its region and currency out as their component
    fields: neither core type has a Python facade, and defaulting the currency
    metadata would put made-up values on the index.
    """

    def __init__(
        self,
        family_name: str,
        region_name: str,
        region_code: str,
        revised: bool,
        frequency: Frequency,
        availability_lag: Period,
        currency_name: str,
        currency_code: str,
        currency_numeric_code: int,
        currency_symbol: str,
        currency_fraction_symbol: str,
        currency_fractions_per_unit: int,
        settings: Settings,
    ) -> None:
        """Build a quoted year-on-year index, keeping its own fixing history.

        The rate is published in its own right rather than derived from a price
        index, so fixings are filed here through add_fixing.

        Args:
            family_name (str): The index family the fixings are stored under.
            region_name (str): The name of the region the index measures.
            region_code (str): The region's code.
            revised (bool): Whether the published figures are subject to
                revision.
            frequency (Frequency): How often the index publishes.
            availability_lag (Period): How long after a period ends its figure
                is published.
            currency_name (str): The currency's name.
            currency_code (str): The currency's ISO 4217 three-letter code.
            currency_numeric_code (int): The currency's ISO 4217 numeric code.
            currency_symbol (str): The currency's symbol.
            currency_fraction_symbol (str): The symbol of the currency's
                fractional unit.
            currency_fractions_per_unit (int): How many fractional units make
                one currency unit.
            settings (Settings): The explicit settings supplying the evaluation
                date and the stored fixings.
        """
        ...
    @staticmethod
    def from_underlying(underlying: ZeroInflationIndex) -> YoYInflationIndex:
        """Build a ratio index dividing a price index's figure by its figure a year earlier.

        The metadata is inherited bar the family name, which is prefixed YYR_,
        so a "UK RPI" underlying yields "UK YYR_RPI"; fixings belong on the
        underlying.

        Args:
            underlying (ZeroInflationIndex): The price index whose consecutive
                figures the rate is derived from.

        Returns:
            YoYInflationIndex: The ratio index, over an empty curve handle.
        """
        ...
    def name(self) -> str:
        """Return the index name, under which fixings are stored.

        Returns:
            str: The name, e.g. "UK YYR_RPI".
        """
        ...
    def ratio(self) -> bool:
        """Return whether this index is the ratio of two price-index fixings.

        Returns:
            bool: True for a ratio index, False for a quoted rate.
        """
        ...
    def underlying_index(self) -> ZeroInflationIndex | None:
        """Return the price index a ratio index divides, None on a quoted one.

        This is the very object from_underlying was handed, not a fresh facade
        around the same core index: a rebuilt one would carry a relinkable
        handle this index never sees, so linking it would silently forecast off
        nothing.

        Returns:
            ZeroInflationIndex | None: The underlying price index, or None.
        """
        ...
    def add_fixing(self, fixing_date: Date, value: float) -> None:
        """Record a published year-on-year rate across the whole inflation period.

        A ratio index reads the underlying's history, so filing here records a
        figure it will never consult.

        Args:
            fixing_date (Date): Any date inside the inflation period the rate
                describes.
            value (float): The published year-on-year rate.

        Raises:
            ItofinError: If the index frequency has no expressible inflation
                period, or a different figure is already stored on a date in
                that period.
        """
        ...
    def fixing(self, fixing_date: Date, forecast_todays_fixing: bool = False) -> float:
        """Return the rate at fixing_date, stored or forecast off the linked curve.

        Args:
            fixing_date (Date): The date the rate is read or forecast for.
            forecast_todays_fixing (bool): Accepted and ignored, as in the core:
                needs_forecast alone decides between history and forecast.

        Returns:
            float: The year-on-year inflation rate.

        Raises:
            ItofinError: If a forecast is asked for with no curve linked.
        """
        ...
    def last_fixing_date(self) -> Date:
        """Return the first day of the period the latest figure on record describes.

        Read off the underlying on a ratio index.

        Returns:
            Date: The start of that inflation period.

        Raises:
            ItofinError: If the index has no fixing history.
        """
        ...
    def link_to(self, curve: YoYInflationTermStructure) -> None:
        """Point the index at curve, so every forecast from here on reads it.

        Takes the YoYInflationTermStructure base, so any subclass links. It is
        the curve behind that facade's handle at call time that is stored, not
        the handle itself.

        Args:
            curve (YoYInflationTermStructure): The curve forecasts are read off.

        Raises:
            ItofinError: If curve somehow carries no link.
        """
        ...
    def needs_forecast(self, fixing_date: Date) -> bool:
        """Return whether fixing_date has to be forecast rather than read from history.

        A ratio index defers the question to its underlying.

        Args:
            fixing_date (Date): The date in question.

        Returns:
            bool: True if the date has to be forecast off the curve.

        Raises:
            ItofinError: If the evaluation date is unset, or the index frequency
                has no expressible inflation period.
        """
        ...
    def __repr__(self) -> str:
        """Return the printable representation.

        Returns:
            str: A string of the form YoYInflationIndex(UK YYR_RPI).
        """
        ...
