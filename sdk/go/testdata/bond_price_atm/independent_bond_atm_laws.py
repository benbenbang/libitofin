from datetime import date, timedelta
from decimal import Decimal, localcontext


def D(value):
    return Decimal(str(value))


def solve(case, price_kind, price):
    with localcontext() as ctx:
        ctx.prec = 100
        settlement = date.fromisoformat(case['settlement'])
        reference = date.fromisoformat(case['reference'])
        dates = [date.fromisoformat(x) for x in case['dates']]
        balances = [D(x) for x in case['balances']]
        rate = D(case['discount_rate'])
        discount = lambda day: (-rate * D((day - reference).days) / D(365)).exp()
        if case['kind'] == 'zero':
            current = balances[0] if settlement < dates[-1] else D(0)
        else:
            current = next((balances[i] for i in range(len(balances)) if settlement < dates[i + 1]), D(0))
        if current == 0:
            return {'status': 'error', 'domain': 'nontradable_current_notional_zero'}
        accrued = D(0)
        principal = D(0)
        annuity = D(0)
        coupon_npv = D(0)
        cashflows = []
        if case['kind'] == 'zero':
            principal = balances[0] * D(case['redemption']) / D(100) * discount(dates[-1])
            cashflows.append({'date': dates[-1].isoformat(), 'principal': str(balances[0] * D(case['redemption']) / D(100)), 'df_reference': str(discount(dates[-1]))})
        else:
            rates = [D(x) for x in case['coupon_rates']]
            for i, nominal in enumerate(balances):
                start, end = dates[i:i + 2]
                coupon_rate = rates[min(i, len(rates) - 1)]
                accrual = D((end - start).days) / D(360)
                ex_date = end - timedelta(days=case['ex_days'])
                ex = case['ex_days'] > 0 and ex_date <= settlement < end
                if start <= settlement < end:
                    accrued = coupon_rate * nominal * (D((settlement - start).days) / D(360) - (accrual if ex else D(0))) * D(100) / current
                if end <= settlement:
                    continue
                reduction = nominal - (balances[i + 1] if i + 1 < len(balances) else D(0))
                redemption = reduction * D(case['redemption']) / D(100)
                principal += redemption * discount(end)
                if not ex:
                    annuity += nominal * accrual * discount(end)
                    coupon_npv += nominal * coupon_rate * accrual * discount(end)
                cashflows.append({'date': end.isoformat(), 'nominal': str(nominal), 'accrual_Actual360': str(accrual), 'coupon_amount': str(nominal * coupon_rate * accrual), 'coupon_included': not ex, 'principal': str(redemption), 'df_reference': str(discount(end))})
        dirty = D(price) + (accrued if price_kind == 'clean' else D(0))
        target_settlement_npv = dirty / D(100) * current
        target_reference_npv = target_settlement_npv * discount(settlement)
        residual = target_reference_npv - principal
        if residual == 0:
            result = {'status': 'ok', 'atm': '0'}
        elif annuity == 0:
            result = {'status': 'error', 'domain': 'null_bps_nonzero_target_minus_principal'}
        else:
            result = {'status': 'ok', 'atm': str(residual / annuity)}
        return {**result, 'current_notional': str(current), 'accrued_per100_current': str(accrued), 'dirty_price_per100': str(dirty), 'settlement_npv': str(target_settlement_npv), 'reference_npv': str(target_reference_npv), 'principal_reference_npv': str(principal), 'coupon_annuity_reference': str(annuity), 'own_dirty_per100': str((principal + coupon_npv) / discount(settlement) * D(100) / current), 'remaining_cashflows': cashflows}
