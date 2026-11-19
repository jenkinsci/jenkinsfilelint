pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo 'a' { x() }
            }
        }
    }
}
