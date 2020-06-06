from flask import Flask
from flask_sqlalchemy import SQLAlchemy

app = Flask(__name__)
app.config.from_object(__name__)
app.config['SECRET_KEY'] = 'e38425d0dcea98645bee3b0f3f3aaf5b'
app.config['SQLALCHEMY_DATABASE_URI'] = 'sqlite:///site.db'
db = SQLAlchemy(app)


# Load goes here to avoid circular dependency when loading `app` variable.
from portunus import routes