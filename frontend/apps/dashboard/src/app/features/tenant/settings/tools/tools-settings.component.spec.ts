import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideTaiga } from '@taiga-ui/core';
import { of, throwError } from 'rxjs';
import { BuiltinToolSetting, TenantDefinedTool } from '../../../../core/api/tenant-api.models';
import { ToolsSettingsApiService } from './tools-settings-api.service';
import { ToolsSettingsComponent } from './tools-settings.component';

const MOCK_BUILTIN: BuiltinToolSetting[] = [
  {
    name: 'lookup_customer',
    description: 'Look up the conversation customer profile',
    classification: 'auto',
    enabled: false,
    requireApproval: false,
    effectiveApproval: false,
  },
  {
    name: 'update_customer_contact',
    description: 'Update customer contact fields',
    classification: 'approval',
    enabled: false,
    requireApproval: false,
    effectiveApproval: true,
  },
];

const MOCK_TENANT: TenantDefinedTool[] = [
  {
    id: 'tool-1',
    name: 'check_order_status',
    description: 'Check order status',
    inputSchema: { type: 'object', properties: { orderId: { type: 'string' } } },
    endpointUrl: 'https://api.example.com/tools/orders',
    hasCredential: true,
    classification: 'approval',
    enabled: true,
    createdAt: '2025-01-01T00:00:00Z',
    updatedAt: '2025-01-01T00:00:00Z',
  },
];

const MOCK_NEW_TOOL: TenantDefinedTool = {
  id: 'tool-2',
  name: 'refund_order',
  description: 'Refund an order',
  inputSchema: { type: 'object' },
  endpointUrl: 'https://api.example.com/tools/refund',
  hasCredential: false,
  classification: 'approval',
  enabled: true,
  createdAt: '2025-01-02T00:00:00Z',
  updatedAt: '2025-01-02T00:00:00Z',
};

describe('ToolsSettingsComponent', () => {
  async function setup(config?: {
    builtin?: BuiltinToolSetting[];
    tenantDefined?: TenantDefinedTool[];
  }) {
    const mockApi = {
      getTools: vi.fn().mockReturnValue(
        of({
          builtin: config?.builtin ?? MOCK_BUILTIN,
          tenantDefined: config?.tenantDefined ?? MOCK_TENANT,
        }),
      ),
      updateBuiltinPolicy: vi.fn().mockReturnValue(of(undefined)),
      createTenantTool: vi.fn().mockReturnValue(of(MOCK_NEW_TOOL)),
      updateTenantTool: vi.fn().mockReturnValue(of(MOCK_TENANT[0])),
      deleteTenantTool: vi.fn().mockReturnValue(of(undefined)),
    };

    TestBed.configureTestingModule({
      imports: [ToolsSettingsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: ToolsSettingsApiService, useValue: mockApi },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ToolsSettingsComponent);
    fixture.detectChanges();
    return { fixture, mockApi };
  }

  it('loads and displays built-in and tenant-defined tools', async () => {
    const { fixture } = await setup();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('lookup_customer');
      expect(fixture.nativeElement.textContent).toContain('check_order_status');
    });
  });

  it('shows loading state then content', async () => {
    const { fixture } = await setup();
    expect(fixture.nativeElement.querySelector('.skeleton')).toBeTruthy();
    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.querySelector('.builtin-grid')).toBeTruthy();
    });
  });

  it('shows error state and retries', async () => {
    const mockApi = {
      getTools: vi.fn().mockReturnValue(throwError(() => new Error('Network error'))),
      updateBuiltinPolicy: vi.fn().mockReturnValue(of(undefined)),
      createTenantTool: vi.fn().mockReturnValue(of(MOCK_NEW_TOOL)),
      updateTenantTool: vi.fn().mockReturnValue(of(MOCK_TENANT[0])),
      deleteTenantTool: vi.fn().mockReturnValue(of(undefined)),
    };
    TestBed.configureTestingModule({
      imports: [ToolsSettingsComponent],
      providers: [
        provideTaiga(),
        provideZonelessChangeDetection(),
        { provide: ToolsSettingsApiService, useValue: mockApi },
      ],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(ToolsSettingsComponent);
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('Network error');
    });

    mockApi.getTools.mockReturnValue(of({ builtin: MOCK_BUILTIN, tenantDefined: MOCK_TENANT }));
    const retryBtn = fixture.nativeElement.querySelector('button');
    retryBtn?.click();
    fixture.detectChanges();

    await vi.waitFor(() => {
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('lookup_customer');
    });
  });

  describe('built-in toggles', () => {
    it('enables a built-in tool when toggle is clicked', async () => {
      const { fixture, mockApi } = await setup();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('lookup_customer');
      });

      const checkboxes = fixture.nativeElement.querySelectorAll(
        '.builtin-grid .toggle input[type="checkbox"]',
      );
      const firstEnabled = checkboxes[0];
      firstEnabled.click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        expect(mockApi.updateBuiltinPolicy).toHaveBeenCalledWith('lookup_customer', true, false);
      });
    });

    it('disables require_approval toggle for platform-approval tools (tighten-only)', async () => {
      const { fixture } = await setup();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('update_customer_contact');
      });

      const sections = fixture.nativeElement.querySelectorAll('.builtin-grid app-dashboard-card');
      const approvalSection = sections[1];

      const toggleLabels = approvalSection.querySelectorAll('.toggle');
      const approvalToggle = toggleLabels[1];
      expect(approvalToggle.classList.contains('toggle--disabled')).toBe(true);
      const checkbox = approvalToggle.querySelector('input[type="checkbox"]');
      expect(checkbox.disabled).toBe(true);
    });
  });

  describe('tenant-defined tools', () => {
    it('opens create dialog, submits, and adds new tool', async () => {
      const { fixture, mockApi } = await setup();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('check_order_status');
      });

      const addBtn = (
        Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLElement[]
      ).find((b) => b.textContent?.trim() === 'Add tool');
      expect(addBtn).toBeTruthy();
      (addBtn as HTMLElement).click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('Add custom tool');
      });

      const nameInput = fixture.nativeElement.querySelector('#tool-name') as HTMLInputElement;
      const descInput = fixture.nativeElement.querySelector('#tool-desc') as HTMLInputElement;
      const schemaInput = fixture.nativeElement.querySelector(
        '#tool-schema',
      ) as HTMLTextAreaElement;
      const urlInput = fixture.nativeElement.querySelector('#tool-url') as HTMLInputElement;

      nameInput.value = 'refund_order';
      nameInput.dispatchEvent(new Event('input'));
      descInput.value = 'Refund an order';
      descInput.dispatchEvent(new Event('input'));
      schemaInput.value = '{"type":"object"}';
      schemaInput.dispatchEvent(new Event('input'));
      urlInput.value = 'https://api.example.com/tools/refund';
      urlInput.dispatchEvent(new Event('input'));
      fixture.detectChanges();

      const submitBtn = (
        Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLElement[]
      ).find((b) => b.textContent?.trim() === 'Create tool');
      (submitBtn as HTMLElement).click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        expect(mockApi.createTenantTool).toHaveBeenCalledWith(
          expect.objectContaining({ name: 'refund_order' }),
        );
      });
    });

    it('opens edit dialog with pre-filled data', async () => {
      const { fixture } = await setup();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('check_order_status');
      });

      const editBtn = (
        Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLElement[]
      ).find((b) => b.textContent?.trim() === 'Edit');
      (editBtn as HTMLElement).click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('Edit tool');
        const nameInput = fixture.nativeElement.querySelector('#tool-name') as HTMLInputElement;
        expect(nameInput.value).toBe('check_order_status');
      });
    });

    it('deletes a tenant-defined tool after confirmation', async () => {
      const { fixture, mockApi } = await setup();
      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('check_order_status');
      });

      const deleteBtn = (
        Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLElement[]
      ).find((b) => b.textContent?.trim() === 'Delete');
      (deleteBtn as HTMLElement).click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        fixture.detectChanges();
        expect(fixture.nativeElement.textContent).toContain('Are you sure');
        expect(fixture.nativeElement.querySelector('.dialog-actions')).toBeTruthy();
      });

      const confirmBtn = fixture.nativeElement.querySelector(
        '.dialog-actions .btn--danger',
      ) as HTMLElement | null;
      expect(confirmBtn).toBeTruthy();
      (confirmBtn as HTMLElement).click();
      fixture.detectChanges();

      await vi.waitFor(() => {
        expect(mockApi.deleteTenantTool).toHaveBeenCalledWith('tool-1');
      });
    });
  });
});
