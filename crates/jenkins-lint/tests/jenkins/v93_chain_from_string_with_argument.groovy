pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script { def a = "x" foo 1 }
            }
        }
    }
}
