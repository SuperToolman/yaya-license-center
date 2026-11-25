"use client";
import { useState } from "react";
import {
  Alert,
  Button,
  ButtonGroup,
  Card,
  FieldError,
  Fieldset,
  Form,
  Label,
  Modal,
  Table,
  TextArea,
  TextField,
} from "@heroui/react";
import {
  CommercialFields,
  commercialPayload,
  emptyCommercial,
} from "./CommercialFields";
import { ApplicationSelect } from "./ApplicationSelect";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { useOperationData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type {
  ApplicationProduct,
  ApplicationReleaseSubmission,
  CommercialVersion,
} from "../../../lib/types/applications";
const statusLabels: Record<string, string> = {
  approved: "已通过",
  pending_review: "待审核",
  rejected: "已驳回",
};
export default function ApplicationCommercialModal({
  product,
  onClose,
  onChanged,
}: {
  product: ApplicationProduct;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [draft, setDraft] = useState(emptyCommercial);
  const [release, setRelease] = useState(product.releaseId ?? "");
  const [error, setError] = useState("");
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  const {
    data,
    refresh,
    error: historyError,
    loading,
  } = useOperationData<CommercialVersion[]>(
    `/api/application-products/${encodeURIComponent(product.productId)}/commercial`,
  );
  const {
    data: releases,
    error: releasesError,
    loading: releasesLoading,
  } = useOperationData<ApplicationReleaseSubmission[]>(
    "/api/application-submissions",
  );
  const mutation = useOperationMutation();
  const productReleases =
    releases?.filter((item) => item.productId === product.productId) ?? [];
  async function submit() {
    if (!release || mutation.pending) return;
    setError("");
    try {
      const config = commercialPayload(draft);
      await mutation.run(
        () =>
          operationCommand(
            `/api/application-products/${encodeURIComponent(product.productId)}/commercial`,
            { releaseId: release, config },
          ),
        () => {
          refresh();
          onChanged();
        },
      );
    } catch (error) {
      setError(error instanceof Error ? error.message : "提交失败");
    }
  }
  async function review(id: string, approved: boolean) {
    if (mutation.pending || (!approved && !reason.trim())) return;
    await mutation.run(
      () =>
        operationCommand(
          `/api/application-commercial/${encodeURIComponent(id)}/review`,
          { approved, reason: approved ? "" : reason.trim() },
        ),
      () => {
        setRejectingId(null);
        setReason("");
        refresh();
        onChanged();
      },
    );
  }
  const visibleError = error || mutation.error || historyError || releasesError;
  return (
    <Modal
      isOpen
      onOpenChange={(open) => {
        if (!open && !mutation.pending) onClose();
      }}
    >
      <Modal.Backdrop
        isDismissable={!mutation.pending}
        isKeyboardDismissDisabled={mutation.pending}
      >
        <Modal.Container
          placement="center"
          scroll="inside"
          className="w-full sm:w-full"
        >
          <Modal.Dialog className="max-w-[1120px]">
            <Modal.Header>
              <Modal.Heading>{product.name} · 销售配置</Modal.Heading>
              <Modal.CloseTrigger
                aria-label="关闭"
                isDisabled={mutation.pending}
              />
            </Modal.Header>
            <Modal.Body>
              <Fieldset.Group>
                <Alert>
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Description>
                      价格和分成形成独立审核版本。审核后重新选择发布版本上架才会生效；已成交订单保持原价格和比例。
                    </Alert.Description>
                  </Alert.Content>
                </Alert>
                {visibleError ? (
                  <Alert status="danger" role="alert">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Title>{visibleError}</Alert.Title>
                    </Alert.Content>
                  </Alert>
                ) : null}
                <Form
                  id="application-commercial-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void submit();
                  }}
                >
                  <Fieldset disabled={mutation.pending}>
                    <Fieldset.Group>
                      <ApplicationSelect
                        label="关联发布版本"
                        required
                        value={release}
                        onChange={setRelease}
                        disabled={mutation.pending || releasesLoading}
                        placeholder={
                          releasesLoading
                            ? "正在加载发布版本…"
                            : "选择关联发布版本"
                        }
                        options={productReleases.map((item) => ({
                          id: item.submissionId,
                          label: `v${item.version} · ${statusLabels[item.status] ?? item.status}`,
                        }))}
                        description={
                          !releasesLoading && !productReleases.length
                            ? "暂无可关联的发布版本，请先导入或提交应用。"
                            : undefined
                        }
                      />
                      <CommercialFields
                        value={draft}
                        onChange={setDraft}
                        disabled={mutation.pending}
                      />
                    </Fieldset.Group>
                  </Fieldset>
                </Form>
                {rejectingId ? (
                  <Form
                    onSubmit={(event) => {
                      event.preventDefault();
                      void review(rejectingId, false);
                    }}
                  >
                    <Card variant="secondary">
                      <Card.Header>
                        <Card.Title>驳回商业配置</Card.Title>
                      </Card.Header>
                      <Card.Content>
                        <TextField
                          isRequired
                          isDisabled={mutation.pending}
                          value={reason}
                          onChange={setReason}
                        >
                          <Label>驳回原因</Label>
                          <TextArea
                            rows={2}
                            placeholder="填写需要调整的销售或分成信息"
                          />
                          <FieldError />
                        </TextField>
                      </Card.Content>
                      <Card.Footer>
                        <ButtonGroup>
                          <Button
                            type="submit"
                            variant="danger-soft"
                            isDisabled={!reason.trim()}
                            isPending={mutation.pending}
                          >
                            确认驳回
                          </Button>
                          <Button
                            variant="secondary"
                            isDisabled={mutation.pending}
                            onPress={() => {
                              setRejectingId(null);
                              setReason("");
                              mutation.clearError();
                            }}
                          >
                            取消驳回
                          </Button>
                        </ButtonGroup>
                      </Card.Footer>
                    </Card>
                  </Form>
                ) : null}
                <Fieldset>
                  <Fieldset.Legend>商业配置审核记录</Fieldset.Legend>
                  <Table>
                    <Table.ScrollContainer>
                      <Table.Content
                        aria-label="商业配置审核记录"
                        className="min-w-[1040px] table-fixed"
                      >
                        <Table.Header>
                          <Table.Column isRowHeader className="w-[140px]">
                            关联版本
                          </Table.Column>
                          <Table.Column className="w-[120px]">
                            费用
                          </Table.Column>
                          <Table.Column className="w-[120px]">
                            发布者分成
                          </Table.Column>
                          <Table.Column className="w-[100px]">
                            状态
                          </Table.Column>
                          <Table.Column className="w-[320px]">
                            说明与审核意见
                          </Table.Column>
                          <Table.Column className="w-[240px]">
                            操作
                          </Table.Column>
                        </Table.Header>
                        <Table.Body
                          renderEmptyState={() => (
                            <p>
                              {loading
                                ? "正在加载审核记录…"
                                : "暂无商业配置审核记录。"}
                            </p>
                          )}
                        >
                          {(data ?? []).map((item) => (
                            <Table.Row key={item.id} id={item.id}>
                              <Table.Cell className="align-middle break-all">
                                {productReleases.find(
                                  (release) =>
                                    release.submissionId === item.releaseId,
                                )?.version
                                  ? `v${productReleases.find((release) => release.submissionId === item.releaseId)?.version}`
                                  : item.releaseId}
                              </Table.Cell>
                              <Table.Cell className="align-middle">
                                {item.config.pricingMode === "free"
                                  ? "免费"
                                  : `¥${(item.config.priceCents / 100).toFixed(2)}`}
                              </Table.Cell>
                              <Table.Cell className="align-middle">
                                {(
                                  item.config.publisherRateBasisPoints / 100
                                ).toFixed(2)}
                                %
                              </Table.Cell>
                              <Table.Cell className="align-middle">
                                <StatusChip status={item.status} />
                              </Table.Cell>
                              <Table.Cell className="align-middle break-words">
                                <p>{item.config.description}</p>
                                {item.reason ? (
                                  <p>审核意见：{item.reason}</p>
                                ) : null}
                              </Table.Cell>
                              <Table.Cell className="align-middle">
                                {item.status === "pending_review" ? (
                                  <ButtonGroup size="sm">
                                    <Button
                                      isDisabled={mutation.pending}
                                      onPress={() => void review(item.id, true)}
                                    >
                                      通过商业审核
                                    </Button>
                                    <Button
                                      variant="danger-soft"
                                      isDisabled={mutation.pending}
                                      onPress={() => {
                                        setRejectingId(item.id);
                                        setReason("");
                                        mutation.clearError();
                                      }}
                                    >
                                      驳回
                                    </Button>
                                  </ButtonGroup>
                                ) : (
                                  "—"
                                )}
                              </Table.Cell>
                            </Table.Row>
                          ))}
                        </Table.Body>
                      </Table.Content>
                    </Table.ScrollContainer>
                    <Table.Footer>共 {data?.length ?? 0} 条配置</Table.Footer>
                  </Table>
                </Fieldset>
              </Fieldset.Group>
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant="secondary"
                isDisabled={mutation.pending}
                onPress={onClose}
              >
                关闭
              </Button>
              <Button
                type="submit"
                form="application-commercial-form"
                isDisabled={!release || releasesLoading}
                isPending={mutation.pending}
              >
                提交商业配置
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
