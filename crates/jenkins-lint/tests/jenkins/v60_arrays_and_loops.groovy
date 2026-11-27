pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def a = new String[3]
                    def b = new int[2][3]
                    def l = new ArrayList<Map<String, String>>()
                    for (int i = 0; i < 3; i++) {
                        if (i == 1) {
                            continue
                        }
                        a[i] = "${i}"
                    }
                    while (true) {
                        break
                    }
                    synchronized (this) {
                        l << [k: 'v']
                    }
                }
            }
        }
    }
}
