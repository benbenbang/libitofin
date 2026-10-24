from decimal import Decimal, localcontext

PI = Decimal('3.14159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798214808651')


def D(value):
    return Decimal(str(value))


def phi_cdf(value):
    with localcontext() as ctx:
        ctx.prec = 240
        x = D(value)
        if x >= 24:
            return Decimal(1)
        if x <= -24:
            return Decimal(0)
        term = x
        total = term
        for n in range(1, 4000):
            term *= -(x * x) * D(2 * n - 1) / D(2 * n * (2 * n + 1))
            total += term
            if abs(term) < D('1e-225'):
                return D('0.5') + total / (2 * PI).sqrt()
        raise RuntimeError('Independent normal-CDF Taylor series did not converge')


def point_payoff(family, side, spot, strike, second_strike=None):
    s, k = D(spot), D(strike)
    signed = s - k if side == 'call' else k - s
    if family == 'asset':
        return s if signed > 0 else D(0)
    if family != 'gap':
        raise ValueError('Unknown family')
    h = D(second_strike)
    return (s - h if side == 'call' else h - s) if signed >= 0 else D(0)


def truncated_lognormal(case):
    with localcontext() as ctx:
        ctx.prec = 100
        s, k, r, q, sigma = [D(case[name]) for name in ['spot', 'strike', 'r', 'q', 'vol']]
        t = D(case['days']) / D(360)
        if s <= 0 or k < 0 or sigma < 0 or t < 0:
            return {'status': 'outside_positive_stock_lognormal_domain'}
        discount, dividend = (-r * t).exp(), (-q * t).exp()
        forward = s * ((r - q) * t).exp()
        if t == 0 or sigma == 0:
            triggered = (forward > k if case['side'] == 'call' else forward < k) if case['family'] == 'asset' else (forward >= k if case['side'] == 'call' else forward <= k)
            asset = s * dividend if triggered else D(0)
            cash = discount if triggered else D(0)
            regime = 'deterministic_point_trigger_not_continuous_half_mass_limit'
        elif k == 0:
            asset = s * dividend if case['side'] == 'call' else D(0)
            cash = discount if case['side'] == 'call' else D(0)
            regime = 'positive_stock_zero_trigger_closed_form_fd_may_reject'
        else:
            width = sigma * t.sqrt()
            d1 = ((s / k).ln() + (r - q + sigma * sigma / 2) * t) / width
            d2 = d1 - width
            sign = 1 if case['side'] == 'call' else -1
            asset = s * dividend * phi_cdf(sign * d1)
            cash = discount * phi_cdf(sign * d2)
            regime = 'continuous_positive_vol_lognormal'
        if case['family'] == 'asset':
            value = asset
        else:
            h = D(case['second_strike'])
            value = asset - h * cash if case['side'] == 'call' else h * cash - asset
        return {'status': 'ok', 'regime': regime, 'T_Actual360': str(t), 'asset_leg': str(asset), 'cash_probability_discounted': str(cash), 'npv': str(value)}


def resolve_second_strike(case):
    result = dict(case)
    if result.pop('second_strike_mode', None) == 'zero_expected_gap':
        trial = dict(result, second_strike=0)
        moment = truncated_lognormal(trial)
        with localcontext() as ctx:
            ctx.prec = 100
            exact = D(moment['asset_leg']) / D(moment['cash_probability_discounted'])
            rounded = float(exact)
        result['second_strike'] = rounded
        result['second_strike_derivation_100_digits'] = str(exact)
        result['second_strike_rounded_binary64'] = rounded.hex()
    return result


def log_cell_cases():
    with localcontext() as ctx:
        ctx.prec = 100
        loc = D(100).ln()
        a, b = loc - D('0.1'), loc + D('0.1')
        sa, sb = a.exp(), b.exp()
        return [
            {'id': 'negative-call-cell', 'family': 'gap', 'side': 'call', 'strike': 50, 'second_strike': 150, 'log_a': str(a), 'log_b': str(b), 'expected_accuracy_sign': 'negative', 'point_value': '-50', 'source_expected_behavior': 'Simpson accuracy rejected; inherit grid-point fallback'},
            {'id': 'negative-put-cell', 'family': 'gap', 'side': 'put', 'strike': 150, 'second_strike': 50, 'log_a': str(a), 'log_b': str(b), 'expected_accuracy_sign': 'negative', 'point_value': '-50', 'source_expected_behavior': 'Simpson accuracy rejected; inherit grid-point fallback'},
            {'id': 'cancelling-call-cell', 'family': 'gap', 'side': 'call', 'strike': 50, 'second_strike_decimal': str((sa + sb) / 2), 'log_a': str(a), 'log_b': str(b), 'mathematical_endpoint_payoff_sum': '0', 'binary64_required_observation': 'Record actual rounded endpoint sum and accuracy; do not assert exact zero after float rounding', 'source_expected_behavior': 'When actual accuracy <= machine epsilon, inherit grid-point fallback'},
        ]
