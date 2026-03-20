(
var argv = thisProcess.argv ? [];
var portValue = if(argv.notEmpty and: { argv[0].notNil }) { argv[0].asString } { nil };
var dirtPort = if(portValue.isNil or: { portValue.isEmpty }) { 57120 } { portValue.asInteger };

s.reboot {
    s.options.numBuffers = 1024 * 256;
    s.options.memSize = 8192 * 32;
    s.options.numWireBufs = 2048;
    s.options.maxNodes = 1024 * 32;
    s.options.numOutputBusChannels = 2;
    s.options.numInputBusChannels = 2;

    s.waitForBoot {
        if(~dirt.notNil) { ~dirt.stop; };
        ~dirt = SuperDirt(2, s);
        ~dirt.loadSoundFiles;
        ~dirt.start(dirtPort, 0 ! 12);
        SuperDirt.default = ~dirt;
        ("SuperDirt: listening on port " ++ dirtPort).postln;
    };

    s.latency = 0.3;
};
)
