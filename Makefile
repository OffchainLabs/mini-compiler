CARGORUN = ./target/release/mini
ARBOSDIR = arb_os
BUILTINDIR = builtin
STDDIR = stdlib
ARTIFACTDIR = contracts/artifacts/arbos
CONTRACTDIR = contracts/arbos
TCSRCDIR = $(CONTRACTDIR)/test
TCBUILDDIR = $(ARTIFACTDIR)/test
UPGRADETESTDIR = upgradetests
ACSRCDIR = $(CONTRACTDIR)/builtin
ACBUILDDIR = $(ARTIFACTDIR)/builtin
ARBOS = $(ARBOSDIR)/arbos.mexe

TEMPLATES = $(ARBOSDIR)/contractTemplates.mini
TESTFILES = $(BUILTINDIR)/kvstest.mexe $(STDDIR)/queuetest.mexe $(BUILTINDIR)/arraytest.mexe $(BUILTINDIR)/globaltest.mexe $(STDDIR)/priorityqtest.mexe $(STDDIR)/bytearraytest.mexe $(STDDIR)/keccaktest.mexe $(STDDIR)/biguinttest.mexe $(STDDIR)/rlptest.mexe $(STDDIR)/storageMapTest.mexe $(BUILTINDIR)/maptest.mexe $(STDDIR)/sha256test.mexe $(STDDIR)/ripemd160test.mexe minitests/codeloadtest.mexe $(STDDIR)/fixedpointtest.mexe $(STDDIR)/blstest.mexe
TESTCONTRACTSPURE = $(TCBUILDDIR)/Add.sol/Add.json $(TCBUILDDIR)/Fibonacci.sol/Fibonacci.json $(TCBUILDDIR)/PaymentChannel.sol/PaymentChannel.json $(TCBUILDDIR)/Underfunded.sol/Underfunded.json $(TCBUILDDIR)/ReverterFactory.sol/ReverterFactory.json $(TCBUILDDIR)/Callback.sol/Callback.json
TESTCONTRACTS = $(ACBUILDDIR)/ArbSys.sol/ArbSys.json $(TESTCONTRACTSPURE)
UPGRADEFILES = $(UPGRADETESTDIR)/regcopy_old.mexe $(UPGRADETESTDIR)/regcopy_new.mexe $(UPGRADETESTDIR)/upgrade1_old.mexe $(UPGRADETESTDIR)/upgrade1_new.mexe $(UPGRADETESTDIR)/upgrade2_new.mexe
ARBOSCONTRACTS = $(ACBUILDDIR)/ArbAddressTable.sol/ArbAddressTable.json $(ACBUILDDIR)/ArbBLS.sol/ArbBLS.json $(ACBUILDDIR)/ArbFunctionTable.sol/ArbFunctionTable.json $(ACBUILDDIR)/ArbInfo.sol/ArbInfo.json $(ACBUILDDIR)/ArbOwner.sol/ArbOwner.json $(ACBUILDDIR)/ArbSys.sol/ArbSys.json $(ACBUILDDIR)/ArbosTest.sol/ArbosTest.json $(ACBUILDDIR)/ArbRetryable.sol/ArbRetryable.json

COMPILEFLAGS = -c "arb_os/constants.json" -i "none"
COMPILEFLAGSNOINLINE = -c "arb_os/constants.json"
EXTRACOMPILEFLAGSFORARBOS = -m

all: $(ARBOSCONTRACTS) $(TESTFILES) $(TESTCONTRACTS) $(TEMPLATES) $(UPGRADEFILES) arbos upgrade test
arbos: $(ARBOSDIR)/arbos.mexe
upgrade: $(ARBOSDIR)/upgrade.json
contracts: $(TESTCONTRACTS) $(ARBOSCONTRACTS)

$(ARBOSDIR)/contractTemplates.mini: compiler $(ARBOSCONTRACTS)
	$(CARGORUN) make-templates

$(BUILTINDIR)/kvstest.mexe: compiler $(BUILTINDIR)/kvstest.mini
	$(CARGORUN) compile $(BUILTINDIR)/kvstest.mini -o $(BUILTINDIR)/kvstest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/queuetest.mexe: compiler $(STDDIR)/queuetest.mini
	$(CARGORUN) compile $(STDDIR)/queuetest.mini -o $(STDDIR)/queuetest.mexe $(COMPILEFLAGS) -t

$(BUILTINDIR)/arraytest.mexe: compiler $(BUILTINDIR)/arraytest.mini
	$(CARGORUN) compile $(BUILTINDIR)/arraytest.mini -o $(BUILTINDIR)/arraytest.mexe $(COMPILEFLAGS) -t

$(BUILTINDIR)/globaltest.mexe: compiler $(BUILTINDIR)/globaltest.mini
	$(CARGORUN) compile $(BUILTINDIR)/globaltest.mini -o $(BUILTINDIR)/globaltest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/priorityqtest.mexe: compiler $(STDDIR)/priorityqtest.mini
	$(CARGORUN) compile $(STDDIR)/priorityqtest.mini -o $(STDDIR)/priorityqtest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/storageMapTest.mexe: compiler $(STDDIR)/storageMapTest.mini
	$(CARGORUN) compile $(STDDIR)/storageMapTest.mini -o $(STDDIR)/storageMapTest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/bytearraytest.mexe: compiler $(STDDIR)/bytearraytest.mini
	$(CARGORUN) compile $(STDDIR)/bytearraytest.mini -o $(STDDIR)/bytearraytest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/blstest.mexe: compiler $(BUILTINMAOS) $(STDDIR)/blstest.mini $(STDDIR)
	$(CARGORUN) compile $(STDDIR)/blstest.mini $(STDLIB) -o $(STDDIR)/blstest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/fixedpointtest.mexe: compiler $(STDDIR)/fixedpointtest.mini $(STDDIR)/fixedpoint.mini
	$(CARGORUN) compile $(STDDIR)/fixedpointtest.mini -o $(STDDIR)/fixedpointtest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/bytearraybench.mexe: compiler $(BUILTINMAOS) $(STDDIR)/bytearraybench.mini $(STDLIB)
	$(CARGORUN) compile $(STDDIR)/bytearraybench.mini $(STDLIB) -o $(STDDIR)/bytearraybench.mexe $(COMPILEFLAGS) -t

$(STDDIR)/bufferopcodetest.mexe: compiler $(BUILTINMAOS) $(STDDIR)/bufferopcodetest.mini
	$(CARGORUN) compile $(STDDIR)/bufferopcodetest.mini -o $(STDDIR)/bufferopcodetest.mexe $(COMPILEFLAGS) -t

minitests/codeloadtest.mexe: compiler minitests/codeloadtest.mini
	$(CARGORUN) compile minitests/codeloadtest.mini -o minitests/codeloadtest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/keccaktest.mexe: compiler $(STDDIR)/keccaktest.mini $(STDDIR)/keccak.mini $(STDDIR)/bytearray.mini $(STDDIR)/expandingIntArray.mini
	$(CARGORUN) compile $(STDDIR)/keccaktest.mini -o $(STDDIR)/keccaktest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/biguinttest.mexe: compiler $(STDDIR)/biguinttest.mini $(STDDIR)/biguint.mini
	$(CARGORUN) compile $(STDDIR)/biguinttest.mini -o $(STDDIR)/biguinttest.mexe $(COMPILEFLAGS) -t

$(STDDIR)/sha256test.mexe: compiler $(STDDIR)/sha256test.mini $(STDDIR)/sha256.mini
	$(CARGORUN) compile $(STDDIR)/sha256test.mini -o $(STDDIR)/sha256test.mexe $(COMPILEFLAGS) -t

$(STDDIR)/ripemd160test.mexe: compiler $(STDDIR)/ripemd160test.mini $(STDDIR)/ripemd160.mini
	$(CARGORUN) compile $(STDDIR)/ripemd160test.mini -o $(STDDIR)/ripemd160test.mexe $(COMPILEFLAGS) -t

$(STDDIR)/rlptest.mexe: compiler $(BUILTINMAOS) $(STDDIR)/rlptest.mini
	$(CARGORUN) compile $(STDDIR)/rlptest.mini -o $(STDDIR)/rlptest.mexe $(COMPILEFLAGS) -t

$(UPGRADETESTDIR)/regcopy_old.mexe: compiler $(UPGRADETESTDIR)/regcopy_old.mini
	$(CARGORUN) compile $(UPGRADETESTDIR)/regcopy_old.mini -o $(UPGRADETESTDIR)/regcopy_old.mexe $(COMPILEFLAGS) -t

$(UPGRADETESTDIR)/regcopy_new.mexe: compiler $(UPGRADETESTDIR)/regcopy_new.mini
	$(CARGORUN) compile $(UPGRADETESTDIR)/regcopy_new.mini -o $(UPGRADETESTDIR)/regcopy_new.mexe $(COMPILEFLAGS) -t

$(UPGRADETESTDIR)/upgrade1_old.mexe: compiler $(UPGRADETESTDIR)/upgrade1_old.mini $(STDDIR)/avmcodebuilder.mini
	$(CARGORUN) compile $(UPGRADETESTDIR)/upgrade1_old.mini -o $(UPGRADETESTDIR)/upgrade1_old.mexe $(COMPILEFLAGS) -t

$(UPGRADETESTDIR)/upgrade1_new.mexe: compiler $(UPGRADETESTDIR)/upgrade1_new.mini
	$(CARGORUN) compile $(UPGRADETESTDIR)/upgrade1_new.mini -o $(UPGRADETESTDIR)/upgrade1_new.mexe $(COMPILEFLAGSNOINLINE)

$(UPGRADETESTDIR)/upgrade2_old.mexe: compiler $(UPGRADETESTDIR)/upgrade2_old.mini $(STDDIR)/avmcodebuilder.mini
	$(CARGORUN) compile $(UPGRADETESTDIR)/upgrade2_old.mini -o $(UPGRADETESTDIR)/upgrade2_old.mexe $(COMPILEFLAGS) -t

$(UPGRADETESTDIR)/upgrade2_new.mexe: compiler $(UPGRADETESTDIR)/upgrade2_old.mexe $(UPGRADETESTDIR)/upgrade2_new.mini $(UPGRADETESTDIR)/impl2.mini $(UPGRADETESTDIR)/upgrade2.toml
	$(CARGORUN) compile $(UPGRADETESTDIR)/upgrade2_new.mini -o $(UPGRADETESTDIR)/upgrade2_new.mexe $(COMPILEFLAGSNOINLINE)
	$(CARGORUN) gen-upgrade-code $(UPGRADETESTDIR)/upgrade2_old.mexe $(UPGRADETESTDIR)/upgrade2_new.mexe $(UPGRADETESTDIR)/bridge2.mini impl2 $(UPGRADETESTDIR)/upgrade2.toml
	$(CARGORUN) compile $(UPGRADETESTDIR)/upgrade2_new.mini -o $(UPGRADETESTDIR)/upgrade2_new.mexe $(COMPILEFLAGSNOINLINE)

$(ARBOSDIR)/arbos-upgrade.mexe: compiler $(TESTCONTRACTS) $(ARBOSDIR) $(STDDIR) $(BUILTINDIR) $(TEMPLATES) src/compile/miniconstants.rs
	cp $(ARBOSDIR)/dummy_version_bridge.mini $(ARBOSDIR)/bridge_arbos_versions.mini
	$(CARGORUN) compile "arb_os" -o "arb_os/arbos-upgrade.mexe" $(COMPILEFLAGS) $(EXTRACOMPILEFLAGSFORARBOS)
	$(CARGORUN) gen-upgrade-code $(ARBOSDIR)/arbos_before.mexe $(ARBOSDIR)/arbos-upgrade.mexe $(ARBOSDIR)/bridge_arbos_versions.mini customize_arbos_bridge_versions $(ARBOSDIR)/upgrade.toml
	$(CARGORUN) compile "arb_os" -o "arb_os/arbos-upgrade.mexe" $(COMPILEFLAGS) $(EXTRACOMPILEFLAGSFORARBOS)
	$(CARGORUN) gen-upgrade-code $(ARBOSDIR)/arbos_before.mexe $(ARBOSDIR)/arbos-upgrade.mexe $(ARBOSDIR)/bridge_arbos_versions.mini customize_arbos_bridge_versions $(ARBOSDIR)/upgrade.toml
	$(CARGORUN) compile "arb_os" -o "arb_os/arbos-upgrade.mexe" $(COMPILEFLAGS) $(EXTRACOMPILEFLAGSFORARBOS)
	cp $(ARBOSDIR)/bridge_arbos_versions.mini $(ARBOSDIR)/save_bridge_for_debugging.mini
	cp $(ARBOSDIR)/dummy_version_bridge.mini $(ARBOSDIR)/bridge_arbos_versions.mini

$(ARBOSDIR)/upgrade.json: compiler $(ARBOSDIR)/arbos-upgrade.mexe
	$(CARGORUN) serialize-upgrade $(ARBOSDIR)/arbos-upgrade.mexe >$(ARBOSDIR)/upgrade.json

$(BUILTINDIR)/maptest.mexe: compiler $(BUILTINMAOS) $(BUILTINDIR)/maptest.mini
	$(CARGORUN) compile $(BUILTINDIR)/maptest.mini -o $(BUILTINDIR)/maptest.mexe $(COMPILEFLAGS) -t

$(ARBOSDIR)/arbos.mexe: compiler $(TESTCONTRACTS) $(ARBOSDIR) $(STDDIR) $(BUILTINDIR) $(TEMPLATES) src/compile/miniconstants.rs
	$(CARGORUN) compile "arb_os" -o "arb_os/arbos.mexe" $(COMPILEFLAGS) $(EXTRACOMPILEFLAGSFORARBOS)

$(TESTCONTRACTSPURE): $(TCSRCDIR)
	(cd contracts; yarn install; yarn build)

$(ARBOSCONTRACTS): $(ACSRCDIR)
	(cd contracts; yarn install; yarn build)

compiler:
	cargo build --release

run: compiler
	$(CARGORUN) run "arb_os/arbos.mexe"

test:
	cargo test --release 

evmtest: $(ARBOS)
	$(CARGORUN) evm-tests

evmtestlogs: compiler $(ARBOS)
	rm -rf evm-test-logs
	mkdir evm-test-logs
	$(CARGORUN) evm-tests --savelogs

testlogs: compiler $(TEMPLATES) $(ARBOS)
	rm -rf testlogs
	mkdir testlogs
	$(CARGORUN) make-test-logs >/dev/null

evmdebug: all
	$(CARGORUN) evm-debug

benchmark: compiler $(TEMPLATES) $(ARBOS)
	$(CARGORUN) make-benchmarks

clean:
	rm -f $(BUILTINDIR)/*.mexe $(STDDIR)/*.mexe $(UPGRADETESTDIR)/*.mexe $(ARBOSDIR)/arbos.mexe $(ARBOSDIR)/arbos-upgrade.mexe $(ARBOSDIR)/upgrade.json minitests/*.mexe $(ARBOSDIR)/contractTemplates.mini
	rm -rf contracts/artifacts contracts/cache
