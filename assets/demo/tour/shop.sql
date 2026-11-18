CREATE TABLE collections (id INTEGER PRIMARY KEY, name TEXT NOT NULL, season TEXT NOT NULL);
INSERT INTO collections VALUES
    (1, 'Everyday goods', 'Core'), (2, 'Weekend field kit', 'Spring'),
    (3, 'Desk essentials', 'Core'), (4, 'Kitchen shelf', 'Autumn');

CREATE TABLE products (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    sku TEXT NOT NULL,
    stock INTEGER NOT NULL,
    collection_id INTEGER REFERENCES collections(id)
);
INSERT INTO products VALUES
    (101, 'Canvas Tote', 'BAG-01', 24, 1),
    (102, 'Pocket Journal', 'PPR-02', 38, 3),
    (103, 'Coffe Mug', 'CER-03', 16, 4),
    (104, 'Linen Apron', 'LIN-04', 12, 4),
    (105, 'Brass Clip', 'DSK-05', 32, 3),
    (106, 'Field Flask', 'OUT-06', 18, 2);

CREATE TABLE orders (id INTEGER PRIMARY KEY, status TEXT NOT NULL);
CREATE TABLE order_items (order_id INTEGER REFERENCES orders(id), product_id INTEGER REFERENCES products(id), quantity INTEGER NOT NULL);
CREATE TABLE shipments (id INTEGER PRIMARY KEY, order_id INTEGER REFERENCES orders(id));
CREATE VIEW inventory AS SELECT sku, name, stock FROM products;

ALTER TABLE products ADD COLUMN published boolean NOT NULL DEFAULT true;
CREATE UNIQUE INDEX products_sku_idx ON products (sku);
ALTER TABLE products ENABLE ROW LEVEL SECURITY;
CREATE POLICY published_catalog ON products FOR SELECT TO PUBLIC USING (published);
COMMENT ON TABLE products IS 'Northline retail catalog';
INSERT INTO orders VALUES (201, 'packed'), (202, 'shipped');
INSERT INTO order_items VALUES (201, 103, 2), (202, 101, 1);
