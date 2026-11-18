pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x ${def v = 1; v} y"
            }
        }
    }
}
