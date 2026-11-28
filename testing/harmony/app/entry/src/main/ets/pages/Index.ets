import { hilog } from '@kit.PerformanceAnalysisKit';
import { startHttpServer } from 'librong_harmony_smoke.so';

const DOMAIN = 0x0000;
const TAG = 'RongSmoke';
const SERVER_PORT = 18080;

interface ServerStatus {
  ok: boolean;
  status?: string;
  port?: number;
  error?: string;
}

@Entry
@Component
struct Index {
  @State statusText: string = 'Starting HTTP test server...';
  @State detailText: string = '';
  @State errorText: string = '';

  aboutToAppear() {
    try {
      const statusJson: string = startHttpServer(SERVER_PORT);
      const status = JSON.parse(statusJson) as ServerStatus;

      if (!status.ok) {
        throw new Error(status.error ?? 'unknown server error');
      }

      const port = status.port ?? SERVER_PORT;
      this.statusText = 'HTTP test server is ready';
      this.detailText = `listen = 0.0.0.0:${port}`;
      hilog.info(DOMAIN, TAG, `startHttpServer: ${statusJson}`);
    } catch (error) {
      const message: string = `${error}`;
      this.errorText = message;
      this.statusText = 'HTTP test server failed';
      hilog.error(DOMAIN, TAG, `startHttpServer failed: ${message}`);
    }
  }

  build() {
    Scroll() {
      Column({ space: 18 }) {
        Text('Rong JSVM Harmony Test App')
          .fontSize(28)
          .fontWeight(FontWeight.Bold)
          .width('100%')

        Text('This app is a thin HarmonyOS shell for device-side JSVM tests. The PC forwards a local port, triggers tests over HTTP, and reads the JSON report directly.')
          .fontSize(16)
          .lineHeight(24)
          .fontColor('#334155')
          .width('100%')

        Column({ space: 8 }) {
          Text('Server')
            .fontSize(14)
            .fontWeight(FontWeight.Bold)
            .fontColor('#0f172a')
            .width('100%')

          Text(this.statusText)
            .fontSize(15)
            .fontColor(this.errorText ? '#b91c1c' : '#0f766e')
            .width('100%')

          if (this.detailText) {
            Text(this.detailText)
              .fontSize(14)
              .fontColor('#334155')
              .width('100%')
          }
        }
        .width('100%')
        .padding(16)
        .borderRadius(18)
        .backgroundColor('#f8fafc')

        if (this.errorText) {
          Column({ space: 8 }) {
            Text('Startup Error')
              .fontSize(14)
              .fontWeight(FontWeight.Bold)
              .fontColor('#991b1b')
              .width('100%')

            Text(this.errorText)
              .fontSize(14)
              .lineHeight(22)
              .fontColor('#7f1d1d')
              .width('100%')
          }
          .width('100%')
          .padding(16)
          .borderRadius(18)
          .backgroundColor('#fef2f2')
        }

        Column({ space: 8 }) {
          Text('Flow')
            .fontSize(14)
            .fontWeight(FontWeight.Bold)
            .fontColor('#0f172a')
            .width('100%')

          Text('1. PC starts the app and forwards a local TCP port to the device.')
            .fontSize(14)
            .fontColor('#334155')
            .width('100%')

          Text('2. App keeps a small HTTP server alive inside the Rust library.')
            .fontSize(14)
            .fontColor('#334155')
            .width('100%')

          Text('3. PC POSTs a filter to /run and gets the full JSON report back.')
            .fontSize(14)
            .fontColor('#334155')
            .width('100%')
        }
        .width('100%')
      }
      .width('100%')
      .alignItems(HorizontalAlign.Start)
      .padding(24)
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#ffffff')
  }
}
