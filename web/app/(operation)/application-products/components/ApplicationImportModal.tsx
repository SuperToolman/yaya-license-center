"use client";
import {
  CommercialFields,
  commercialPayload,
  emptyCommercial,
} from "./CommercialFields";
import { useRef, useState } from "react";
import { ArrowUpFromLine, Cube } from "@gravity-ui/icons";
import {
  Alert,
  Button,
  Card,
  Description,
  Fieldset,
  Form,
  Modal,
} from "@heroui/react";
import { OFFICIAL_PUBLISHER } from "../../../lib/application-publisher";
import { ApplicationSelect } from "./ApplicationSelect";
import { operationData } from "../../../lib/operation-api";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import type {
  ApplicationProduct,
  Customer,
} from "../../../lib/operation-types";

export default function ApplicationImportModal({
  customers,
  products,
  onClose,
  onImported,
}: {
  customers: Customer[];
  products: ApplicationProduct[];
  onClose: () => void;
  onImported: () => void;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [publisher, setPublisher] = useState("");
  const [product, setProduct] = useState("");
  const [commercial, setCommercial] = useState(emptyCommercial);
  const [file, setFile] = useState<File>();
  const [validation, setValidation] = useState("");
  const mutation = useOperationMutation();
  async function submit() {
    if (!file || !publisher || mutation.pending) return;
    if (file.size > 64 * 1024 * 1024) {
      setValidation("应用包不能超过 64MiB");
      return;
    }
    setValidation("");
    let config;
    try {
      config = commercialPayload(commercial);
    } catch (e) {
      setValidation(e instanceof Error ? e.message : "商业资料无效");
      return;
    }
    const body = new FormData();
    body.set("commercial", JSON.stringify(config));
    body.append("publisherSubject", publisher);
    body.append("package", file);
    if (product) body.append("productId", product);
    await mutation.run(
      () =>
        operationData("/api/application-products/import", {
          method: "POST",
          body,
        }),
      () => {
        onImported();
        onClose();
      },
    );
  }
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
          <Modal.Dialog className="max-w-[800px]">
            <Modal.Header>
              <Modal.Heading>导入应用包</Modal.Heading>
              <Modal.CloseTrigger
                aria-label="关闭"
                isDisabled={mutation.pending}
              />
            </Modal.Header>
            <Modal.Body>
              <Form
                id="application-import-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void submit();
                }}
              >
                <Fieldset disabled={mutation.pending}>
                  <Fieldset.Group>
                    <Alert status="accent">
                      <Alert.Indicator>
                        <Cube />
                      </Alert.Indicator>
                      <Alert.Content>
                        <Alert.Title>离线应用包导入</Alert.Title>
                        <Alert.Description>
                          支持完整单应用或套应用 V3
                          ZIP，导入后进入待审核，审核通过后另行上架。
                        </Alert.Description>
                      </Alert.Content>
                    </Alert>
                    <ApplicationSelect
                      label="发布客户"
                      required
                      value={publisher}
                      onChange={(value) => {
                        setPublisher(value);
                        setProduct("");
                      }}
                      disabled={mutation.pending}
                      placeholder="选择应用发布方"
                      options={[
                        {
                          id: OFFICIAL_PUBLISHER.subject,
                          label: `${OFFICIAL_PUBLISHER.name}（官方）`,
                        },
                        ...customers
                          .filter(
                            (c) =>
                              c.status === "active" &&
                              c.customerId !== OFFICIAL_PUBLISHER.subject,
                          )
                          .map((c) => ({ id: c.customerId, label: c.name })),
                      ]}
                    />
                    <ApplicationSelect
                      label="目标商品"
                      value={product || "new"}
                      onChange={(value) =>
                        setProduct(value === "new" ? "" : value)
                      }
                      disabled={!publisher || mutation.pending}
                      options={[
                        { id: "new", label: "创建新商品" },
                        ...products
                          .filter((p) => p.publisherSubject === publisher)
                          .map((p) => ({
                            id: p.productId,
                            label: `${p.name} · 新版本`,
                          })),
                      ]}
                      description="更新现有商品须属于同一发布方，版本不能重复，已有成员不能移除。"
                    />
                    <Alert status="accent">
                      <Alert.Content>
                        <Alert.Title>
                          申请类型：{product ? "版本升级" : "新上线"}
                        </Alert.Title>
                        <Alert.Description>
                          {product
                            ? "导入为所选商品的新版本，审核通过后另行选择上架。"
                            : "创建新的应用商品，导入后进入待审核。"}
                        </Alert.Description>
                      </Alert.Content>
                    </Alert>
                    <Card variant="secondary">
                      <Card.Header>
                        <Card.Title>应用包 ZIP</Card.Title>
                        <Card.Description>
                          最大 64MiB，将检查包完整性、依赖闭合及版本规则。
                        </Card.Description>
                      </Card.Header>
                      <Card.Content>
                        <Description>
                          {file
                            ? `${file.name} · ${(file.size / 1024 / 1024).toFixed(2)} MiB`
                            : "尚未选择应用包"}
                        </Description>
                      </Card.Content>
                      <Card.Footer>
                        <input
                          ref={fileInput}
                          aria-label="应用包 ZIP"
                          type="file"
                          accept=".zip,application/zip"
                          disabled={mutation.pending}
                          className="sr-only"
                          onChange={(event) => {
                            setFile(event.target.files?.[0]);
                            setValidation("");
                          }}
                        />
                        <Button
                          variant="secondary"
                          isDisabled={mutation.pending}
                          onPress={() => fileInput.current?.click()}
                        >
                          <ArrowUpFromLine />
                          {file ? "重新选择应用包" : "选择应用包"}
                        </Button>
                      </Card.Footer>
                    </Card>
                    <CommercialFields
                      value={commercial}
                      onChange={setCommercial}
                      disabled={mutation.pending}
                    />
                    {validation || mutation.error ? (
                      <Alert status="danger" role="alert">
                        <Alert.Indicator />
                        <Alert.Content>
                          <Alert.Title>
                            {validation || mutation.error}
                          </Alert.Title>
                        </Alert.Content>
                      </Alert>
                    ) : null}
                  </Fieldset.Group>
                </Fieldset>
              </Form>
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant="secondary"
                isDisabled={mutation.pending}
                onPress={onClose}
              >
                取消
              </Button>
              <Button
                type="submit"
                form="application-import-form"
                isDisabled={!file || !publisher}
                isPending={mutation.pending}
              >
                导入并进入审核
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
