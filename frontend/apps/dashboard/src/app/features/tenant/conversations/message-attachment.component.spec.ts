import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MessageAttachmentComponent } from './message-attachment.component';
import { MessageAttachment } from '../../../core/api/tenant-api.models';

describe('MessageAttachmentComponent', () => {
  function createAttachment(
    overrides: Partial<MessageAttachment> = {},
  ): MessageAttachment {
    return {
      id: 'att-1',
      kind: 'image',
      status: 'stored',
      mimeType: null,
      sizeBytes: null,
      fileName: null,
      url: 'https://example.com/photo.jpg',
      ...overrides,
    };
  }

  async function createComponent(attachment: MessageAttachment) {
    await TestBed.configureTestingModule({
      imports: [MessageAttachmentComponent],
      providers: [provideZonelessChangeDetection()],
    }).compileComponents();

    const fixture = TestBed.createComponent(MessageAttachmentComponent);
    const component = fixture.componentInstance;
    fixture.componentRef.setInput('attachment', attachment);
    fixture.detectChanges();
    return { fixture, component };
  }

  it('renders image with img element and links to url', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'image' }),
    );
    const img = fixture.nativeElement.querySelector('img');
    const link = fixture.nativeElement.querySelector('a');
    expect(img).toBeTruthy();
    expect(img.src).toContain('photo.jpg');
    expect(link.href).toContain('photo.jpg');
  });

  it('renders audio with controls', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'audio' }),
    );
    const audio = fixture.nativeElement.querySelector('audio');
    expect(audio).toBeTruthy();
    expect(audio.controls).toBe(true);
  });

  it('renders video with controls', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'video' }),
    );
    const video = fixture.nativeElement.querySelector('video');
    expect(video).toBeTruthy();
    expect(video.controls).toBe(true);
  });

  it('renders document with file name and size', async () => {
    const { fixture } = await createComponent(
      createAttachment({
        kind: 'document',
        fileName: 'report.pdf',
        sizeBytes: 204800,
      }),
    );
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('report.pdf');
    expect(el.textContent).toContain('KB');
  });

  it('renders document with fallback name when fileName is null', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'document', fileName: null }),
    );
    expect(fixture.nativeElement.textContent).toContain('Document');
  });

  it('shows pending placeholder for pending status', async () => {
    const { fixture } = await createComponent(
      createAttachment({ status: 'pending', url: null }),
    );
    expect(fixture.nativeElement.textContent).toContain('Receiving');
  });

  it('shows failed note for failed status', async () => {
    const { fixture } = await createComponent(
      createAttachment({ status: 'failed', url: null }),
    );
    expect(fixture.nativeElement.textContent).toContain('Failed to load');
  });

  it('renders document link that opens in new tab', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'document' }),
    );
    const link = fixture.nativeElement.querySelector('a');
    expect(link.target).toBe('_blank');
    expect(link.rel).toContain('noopener');
  });

  it('renders image link that opens in new tab', async () => {
    const { fixture } = await createComponent(
      createAttachment({ kind: 'image' }),
    );
    const link = fixture.nativeElement.querySelector('a');
    expect(link.target).toBe('_blank');
    expect(link.rel).toContain('noopener');
  });

  it('formats bytes correctly', async () => {
    const { component } = await createComponent(
      createAttachment({ kind: 'document', sizeBytes: 500 }),
    );
    expect(component['formatSize'](500)).toBe('500 B');
    expect(component['formatSize'](2048)).toBe('2.0 KB');
    expect(component['formatSize'](1572864)).toBe('1.5 MB');
  });
});
