use respire::pir::respire::RespireParamsExpanded;
use respire::pir::respire_harness::FactoryParams;
use respire::{generate_main, respire};

const PARAMS: RespireParamsExpanded = FactoryParams::single_record_256(9, 9).expand().expand();

type ThePIR = respire!(PARAMS);
generate_main!(ThePIR);
