(
s.waitForBoot({
    SynthDef(\fin_bd, { |out = 0, amp = 0.9, pan = 0.0, gate = 1, freq = 55|
        var pitchEnv = EnvGen.kr(Env([freq * 2.6, freq * 1.2, freq], [0.008, 0.08], -6));
        var ampEnv = EnvGen.kr(Env.asr(0.001, 1.0, 0.16, -4), gate, doneAction: Done.freeSelf);
        var click = BPF.ar(Hasher.ar(Sweep.ar), 3200, 0.7) * 0.02;
        var body = SinOsc.ar(pitchEnv, 0, 0.9) + SinOsc.ar(pitchEnv * 0.5, 0, 0.2);
        var signal = LeakDC.ar((body + click) * ampEnv * amp);
        Out.ar(out, Pan2.ar(signal.tanh, pan));
    }).add;

    SynthDef(\fin_sd, { |out = 0, amp = 0.55, pan = -0.1, gate = 1, freq = 180|
        var ampEnv = EnvGen.kr(Env.asr(0.001, 1.0, 0.12, -4), gate, doneAction: Done.freeSelf);
        var snapEnv = EnvGen.kr(Env.perc(0.001, 0.045, 1.0, -6));
        var noise = WhiteNoise.ar(1.0);
        var body = SinOsc.ar(freq, 0, 0.35) + SinOsc.ar(freq * 1.85, 0, 0.12);
        var snap = HPF.ar(noise, 1800) * snapEnv * 0.9;
        var tone = BPF.ar(noise, 3300, 0.6) * 0.25;
        var signal = (body + snap + tone) * ampEnv * amp;
        Out.ar(out, Pan2.ar(signal.tanh, pan));
    }).add;

    SynthDef(\fin_hh, { |out = 0, amp = 0.25, pan = 0.2, gate = 1, freq = 880|
        var ampEnv = EnvGen.kr(Env.asr(0.0005, 1.0, 0.05, -8), gate, doneAction: Done.freeSelf);
        var noise = WhiteNoise.ar(1.0);
        var metal = Mix.fill(6, { |i| Ringz.ar(noise, freq * (i + 1.3), 0.015) * 0.08 });
        var signal = HPF.ar(noise + metal, 5000) * ampEnv * amp;
        Out.ar(out, Pan2.ar(signal.tanh, pan));
    }).add;

    SynthDef(\fin_tone, { |out = 0, amp = 0.4, pan = 0.0, gate = 1, freq = 220|
        var ampEnv = EnvGen.kr(Env.asr(0.002, 1.0, 0.14, -4), gate, doneAction: Done.freeSelf);
        var signal = Mix([
            Saw.ar(freq, 0.12),
            Pulse.ar(freq * 0.5, 0.45, 0.06),
            SinOsc.ar(freq * 2, 0, 0.05)
        ]);
        signal = LPF.ar(signal, (freq * 6).clip(400, 7000));
        Out.ar(out, Pan2.ar(signal * ampEnv * amp, pan));
    }).add;

    "FIN synthdefs loaded: fin_bd, fin_sd, fin_hh, fin_tone".postln;
});
)
