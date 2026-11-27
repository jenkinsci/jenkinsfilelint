pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x ${a # b} y"
            }
        }
    }
}
