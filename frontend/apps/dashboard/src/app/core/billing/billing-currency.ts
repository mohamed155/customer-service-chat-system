/**
 * The currency of every price in the billing screens. The billing API sends a price in cents and
 * no currency, and the plans are priced in US dollars. The symbol, the grouping and where the
 * symbol goes come from the locale (`Intl`), so no screen writes a `$`. When the API gets a
 * currency on the plan, this constant goes and the screens read it from the data.
 */
export const BILLING_CURRENCY = 'USD';

/** The options for `localeNumber` that write a price given in cents. */
export const BILLING_PRICE_FORMAT = { style: 'currency', currency: BILLING_CURRENCY } as const;
