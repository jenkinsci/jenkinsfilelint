pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                def x = 1
                echo "${x}"
            }
        }
    }
}
