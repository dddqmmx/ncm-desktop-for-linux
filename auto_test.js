const { PlayerService } = require('./native/index.node');
const player = new PlayerService();

async function run() {
    console.log("Starting playback...");
    await player.playUrl("https://www.soundhelix.com/examples/mp3/SoundHelix-Song-1.mp3", 0, false);

    let ticks = 0;
    const interval = setInterval(() => {
        const progress = player.progressMs;
        ticks++;
        if (ticks > 50) {
            clearInterval(interval);
            player.stop();
            console.log("Test finished.");
            process.exit(0);
        }
    }, 100);
}

run().catch(console.error);
