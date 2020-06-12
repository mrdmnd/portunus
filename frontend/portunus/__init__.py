from flask import Flask
from config import Config
from flask_sqlalchemy import SQLAlchemy
from flask_migrate import Migrate
from flask_bcrypt import Bcrypt
from flask_login import LoginManager

app = Flask(__name__)
app.config.from_object(Config)
db = SQLAlchemy(app)
migrate = Migrate(app, db)
bcrypt = Bcrypt(app)
login_manager = LoginManager(app)
login_manager.login_view = 'login'
login_manager.login_message_category = 'info'


# Load goes here to avoid circular dependency when loading `app` variable.
from portunus import routes, models

db.drop_all()
db.create_all()


# Add the database instance and models to the shell session invoked with `flask shellj`
@app.shell_context_processor
def make_shell_context():
    return {'db': db, 'User': models.User, 'DungeonRoute': models.DungeonRoute}
