"""Build an independent oracle from the vendored QuantLib CommonVarsON inputs."""
import ctypes
import subprocess
import sys
import tempfile
from pathlib import Path
import QuantLib as ql
import QuantLib._QuantLib as native

source = Path(sys.argv[1])
text = (source / 'test-suite/optionletstripper.cpp').read_text()
common = 'struct CommonVarsON {' + text.split('struct CommonVarsON {')[1].split('BOOST_AUTO_TEST_CASE')[0]
program = r'''
#include <ql/quantlib.hpp>
#include <iostream>
#include <iomanip>
using namespace QuantLib;
COMMON
extern "C" void run() {
    CommonVarsON v;
    Settings::instance().evaluationDate() = v.today;
    v.setSofrHandle(); v.setRealCapFloorVolSurface();
    auto index = ext::make_shared<Sofr>(v.sofrCurveHandle);
    index->addFixing(v.today, .0304);
    auto strip = ext::make_shared<OptionletStripper1>(v.capfloorVol, index,
        Null<Real>(), 1e-6, 100, v.sofrCurveHandle, Normal, 0.0, true, 3*Months);
    auto adapter = ext::make_shared<StrippedOptionletAdapter>(strip);
    auto schedule = Schedule(v.startDate, v.endDate, 3*Months, v.calendar,
        v.convention, v.convention, DateGeneration::Forward, false);
    Leg leg = OvernightLeg(schedule,index).withNotionals(1000000)
        .withPaymentAdjustment(ModifiedFollowing).withPaymentLag(2);
    Cap cap(leg, std::vector<Rate>{.04});
    cap.setPricingEngine(ext::make_shared<BachelierCapFloorEngine>(v.sofrCurveHandle,
        Handle<OptionletVolatilityStructure>(adapter)));
    std::cout << std::setprecision(17) << "overnight_cap " << cap.NPV() << '\n';
    for (double t : {1., 3., 5.})
        std::cout << "overnight_vol " << t << ' ' << adapter->volatility(t,.04) << '\n';
}
'''.replace('COMMON', common)
assert ql.__version__ == '1.43'
with tempfile.TemporaryDirectory() as tmp:
    cpp = Path(tmp) / 'oracle.cpp'; cpp.write_text(program)
    binary = Path(tmp) / 'oracle.so'
    subprocess.run(['clang++', '-std=c++17', '-bundle', '-undefined', 'dynamic_lookup',
                    '-I/opt/homebrew/include', f'-I{source}', str(cpp), '-o', str(binary)], check=True)
    ctypes.CDLL(native.__file__, mode=ctypes.RTLD_GLOBAL)
    ctypes.CDLL(str(binary)).run()
