from flask import Flask
from config import Config
from flask_sqlalchemy import SQLAlchemy
from flask_migrate import Migrate
from flask_bcrypt import Bcrypt
from flask_login import LoginManager

db = SQLAlchemy()
migrate = Migrate()
bcrypt = Bcrypt()
login_manager = LoginManager()
login_manager.login_view = 'auth.login'
login_manager.login_message_category = 'info'


def create_app(config_class=Config):
    app = Flask(__name__)
    app.config.from_object(config_class)
    db.init_app(app)
    migrate.init_app(app, db)
    bcrypt.init_app(app)
    login_manager.init_app(app)

    from portunus.errors import bp as errors_bp
    app.register_blueprint(errors_bp)

    from portunus.auth import bp as auth_bp
    app.register_blueprint(auth_bp, url_prefix='/auth')

    from portunus.main import bp as main_bp
    app.register_blueprint(main_bp)

    return app
