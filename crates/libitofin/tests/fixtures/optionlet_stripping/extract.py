"""Extract the original QuantLib optionlet-stripper market inputs without changes."""
import hashlib
import re
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text()
out = Path(__file__).parent
normal = source.split('void setRealCapFloorTermVolSurface()')[1].split('struct CommonVarsON')[0]
lognormal = source.split('void setCapFloorTermVolSurface()')[1].split('void setRealCapFloorTermVolSurface()')[0]
curves = source.split('void setRealTermStructure()')[1].split('void setFlatTermVolCurve()')[0]

def values(text):
    return re.findall(r'-?\d+(?:\.\d+)?', text)

def array(text, name):
    return re.search(r'\b' + name + r'\s*=\s*\{(.*?)\}', text, re.S).group(1)

for name, text in [('normal', normal), ('lognormal', lognormal)]:
    strikes = values(array(text, 'strikes'))
    if name == 'normal':
        vols = [str(float(v) / 100) for v in values(array(text, 'rawVols'))]
    else:
        vols = re.findall(r'termV\[\d+\]\[\d+\]\s*=\s*([\d.]+)', text)
    rows = [','.join(strikes)] + [','.join(vols[i:i+13]) for i in range(0, len(vols), 13)]
    (out / f'{name}.txt').write_text('\n'.join(rows) + '\n')
for name, data in [('dates', re.findall(r'datesTmp\s*=\s*\{(.*?)\}', curves, re.S)),
                   ('rates', re.findall(r'rates\s*=\s*\{(.*?)\}', curves, re.S))]:
    (out / f'curve_{name}.txt').write_text('\n'.join(','.join(values(row)) for row in data) + '\n')
print(hashlib.sha256(source.encode()).hexdigest())
overnight = source.split('struct CommonVarsON {')[1].split('BOOST_AUTO_TEST_CASE')[0]
(out / 'overnight_vols.txt').write_text('\n'.join(','.join(str(float(v) / 10000) for v in values(row))
    for row in re.findall(r'\{([\d.,\s]+)\}', re.search(r'Real data\[10\]\[3\] = (.*?);', overnight, re.S).group(1))) + '\n')
months = {'Jan': 1, 'Feb': 2, 'Mar': 3, 'Apr': 4, 'May': 5, 'Jun': 6,
          'Jul': 7, 'Aug': 8, 'Sep': 9, 'Oct': 10, 'Nov': 11, 'Dec': 12}
dates = re.findall(r'Date\((\d+), (\w+), (\d+)\)', array(overnight, 'dates'))
rates = re.findall(r'([\d.]+)\s*/\s*100.0', array(overnight, 'zeroRates'))
(out / 'overnight_curve.txt').write_text('\n'.join(
    f'{year},{months[month]},{day},{float(rate)/100}'
    for (day, month, year), rate in zip(dates, rates, strict=True)) + '\n')
